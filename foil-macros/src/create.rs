use crate::{
    attrs::Attrs,
    types::{into_input_type, is_copy, unwrap_option},
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::{collections::BTreeMap, str::FromStr};
use syn::{
    parse2, spanned::Spanned, Data, DataStruct, DeriveInput, Error, Fields, Ident, Lit, LitStr,
    Path, Result, Type, Visibility,
};

pub fn derive_create(input: DeriveInput) -> Result<TokenStream> {
    let dbs = dbs!();
    let config = extract_config(input)?;

    let create = dbs
        .iter()
        .map(|db| expand_create(db, &config))
        .collect::<TokenStream>();

    let input = expand_input(&dbs, &config);

    Ok(quote! {
        #create
        #input
    })
}

struct Config {
    entity_ident: Ident,
    vis: Visibility,
    input_ident: Ident,
    fields: BTreeMap<Ident, FieldConfig>,
}

struct FieldConfig {
    col_name: LitStr,
    default_mode: DefaultMode,
    input_ty: Type,
}

enum DefaultMode {
    None,
    DefaultFn(Path),
    Generated,
}

fn extract_config(input: DeriveInput) -> Result<Config> {
    let input_span = input.span();
    let entity_ident = input.ident;
    let vis = input.vis;
    let input_ident = Ident::new(&format!("{}Input", entity_ident), Span::call_site());
    let mut fields = BTreeMap::new();

    let mut attrs = Attrs::extract(input.attrs)?;

    let (id_field_name, id_field_name_span) = attrs
        .get_name_value("id_field")?
        .map(|lit| {
            if let Lit::Str(lit_str) = lit {
                Ok((
                    Ident::new(&lit_str.value(), Span::call_site()),
                    Some(lit_str.span()),
                ))
            } else {
                Err(Error::new(lit.span(), "expected string literal"))
            }
        })
        .transpose()?
        .unwrap_or_else(|| (Ident::new("id", Span::call_site()), None));

    if let Some(mut lazy) = attrs.get_list("lazy")? {
        let keys = lazy.keys().map(ToOwned::to_owned).collect::<Vec<_>>();
        for name in keys {
            let mut attrs = lazy.get_list(&name)?.unwrap();

            let ty = if let Some(lit) = attrs.get_name_value("type")? {
                if let Lit::Str(lit_str) = lit {
                    parse2(TokenStream::from_str(&lit_str.value())?)?
                } else {
                    return Err(Error::new(lit.span(), "expected string literal"));
                }
            } else {
                return Err(Error::new(
                    name.span(),
                    "type must be specified for lazy column",
                ));
            };

            let config = extract_field_config(&name, &ty, attrs)?;

            fields.insert(name, config);
        }
    }

    attrs.ignore(&["table", "setters"]);
    attrs.done()?;

    if let Data::Struct(DataStruct {
        struct_token: _,
        fields: Fields::Named(fields_named),
        semi_token: _,
    }) = input.data
    {
        for field in fields_named.named {
            let name = field.ident.unwrap();
            let ty = field.ty;

            let attrs = Attrs::extract(field.attrs)?;

            let config = extract_field_config(&name, &ty, attrs)?;

            fields.insert(name, config);
        }

        if let Some(id_field) = fields.get_mut(&id_field_name) {
            if matches!(id_field.default_mode, DefaultMode::None) {
                id_field.default_mode = DefaultMode::Generated;
            }
        } else {
            return Err(Error::new(
                id_field_name_span.unwrap_or(input_span),
                &format!("field {} does not exist", id_field_name),
            ));
        }

        Ok(Config {
            entity_ident,
            vis,
            input_ident,
            fields,
        })
    } else {
        Err(Error::new(input_span, "expected struct with named fields"))
    }
}

fn extract_field_config(name: &Ident, ty: &Type, mut attrs: Attrs) -> Result<FieldConfig> {
    let col_name = attrs
        .get_name_value("rename")?
        .map(|lit| {
            if let Lit::Str(lit_str) = lit {
                Ok(lit_str)
            } else {
                Err(Error::new(lit.span(), "expected string literal"))
            }
        })
        .transpose()?
        .unwrap_or_else(|| LitStr::new(&name.to_string(), Span::call_site()));

    let input_ty = attrs
        .get_name_value("input_type")?
        .map(|lit| {
            if let Lit::Str(lit_str) = lit {
                let ty = parse2(TokenStream::from_str(&lit_str.value())?)?;
                Ok(ty)
            } else {
                Err(Error::new(lit.span(), "expected string literal"))
            }
        })
        .transpose()?
        .unwrap_or_else(|| into_input_type(ty.clone()));

    let mut default_mode = DefaultMode::None;

    if attrs.get_path("default")? {
        default_mode =
            DefaultMode::DefaultFn(parse2(quote! { ::std::default::Default::default }).unwrap());
    }

    if let Some(lit) = attrs.get_name_value("default_with")? {
        if let Lit::Str(lit_str) = lit {
            default_mode =
                DefaultMode::DefaultFn(parse2(TokenStream::from_str(&lit_str.value())?)?);
        } else {
            return Err(Error::new(lit.span(), "expected string literal"));
        }
    }

    if attrs.get_path("generated")? {
        default_mode = DefaultMode::Generated;
    }

    attrs.done()?;

    Ok(FieldConfig {
        col_name,
        default_mode,
        input_ty,
    })
}

fn expand_create(db: &Type, config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let input_ident = &config.input_ident;
    let generated_col_names = config.fields.iter().filter_map(|(_, field_config)| {
        if let DefaultMode::Generated = field_config.default_mode {
            Some(&field_config.col_name)
        } else {
            None
        }
    });
    let field_names = config.fields.keys();
    let construct_field_exprs = config
        .fields
        .iter()
        .map(|(field_name, field_config)| expand_construct_field_expr(field_name, field_config));

    quote! {
        #[automatically_derived]
        impl ::foil::entity::Create<#db> for #entity_ident {
            type Input<'q> = #input_ident<'q>;

            fn generated_col_names() -> &'static [&'static str] {
                &[
                    #(
                        #generated_col_names
                    ),*
                ]
            }

            fn construct<'q>(
                input: &Self::Input<'q>,
                generated: &::foil::manager::Record<#db>,
            ) -> ::std::result::Result<Self, ::foil::manager::RecordError> {
                ::std::result::Result::Ok(Self {
                    #(
                        #field_names: #construct_field_exprs
                    ),*
                })
            }
        }
    }
}

fn expand_construct_field_expr(field_name: &Ident, field_config: &FieldConfig) -> TokenStream {
    let col_name = &field_config.col_name;
    let is_optional = !matches!(field_config.default_mode, DefaultMode::None);

    let alias = if is_optional {
        quote! { val }
    } else {
        quote! { input.#field_name }
    };

    let owned_expr = if is_copy(&field_config.input_ty) {
        alias
    } else if unwrap_option(&mut field_config.input_ty.clone()) {
        quote! { #alias.map(::std::borrow::ToOwned::to_owned) }
    } else {
        quote! { ::std::borrow::ToOwned::to_owned(#alias)}
    };

    match &field_config.default_mode {
        DefaultMode::None => owned_expr,
        DefaultMode::Generated => {
            quote! {
                if let ::foil::entity::Field::Set(val) = input.#field_name {
                    #owned_expr
                } else {
                    generated.col(#col_name)?
                }
            }
        }
        DefaultMode::DefaultFn(path) => {
            quote! {
                if let ::foil::entity::Field::Set(val) = input.#field_name {
                    #owned_expr
                } else {
                    #path()
                }
            }
        }
    }
}

fn expand_input(dbs: &[Type], config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let vis = &config.vis;
    let input_ident = &config.input_ident;
    let field_names = config.fields.keys().collect::<Vec<_>>();
    let field_input_types = config.fields.iter().map(|(_, field_config)| {
        let input_ty = &field_config.input_ty;
        if matches!(field_config.default_mode, DefaultMode::None) {
            quote! { #input_ty }
        } else {
            quote! { ::foil::entity::Field<#input_ty> }
        }
    });
    let field_from_exprs = config
        .fields
        .iter()
        .map(|(field_name, field_config)| expand_from_field_expr(field_name, field_config));
    let to_input_record_entries = config
        .fields
        .iter()
        .map(|(field_name, field_config)| expand_to_input_record_entry(field_name, field_config))
        .collect::<Vec<_>>();

    let to_input_record_impls = dbs
        .iter()
        .map(|db| {
            quote! {
                #[automatically_derived]
                impl<'q> ::foil::manager::ToInputRecord<'q, #db> for #input_ident<'q> {
                    fn to_input_record(&self) -> ::foil::manager::InputRecord<'q, #db> {
                        let mut values = foil::manager::InputRecord::new();
                        #(
                            #to_input_record_entries
                        )*
                        values
                    }
                }
            }
        })
        .collect::<TokenStream>();

    quote! {
        #vis struct #input_ident<'q> {
            #(
                #field_names: #field_input_types
            ),*
        }

        #[automatically_derived]
        impl<'q> ::std::convert::From<&'q #entity_ident> for #input_ident<'q> {
            fn from(from: &'q #entity_ident) -> Self {
                Self {
                    #(
                        #field_names: #field_from_exprs
                    ),*
                }
            }
        }

        #to_input_record_impls
    }
}

fn expand_from_field_expr(field_name: &Ident, field_config: &FieldConfig) -> TokenStream {
    let mut expr = quote! { from.#field_name };

    let mut unwrapped = field_config.input_ty.clone();
    if unwrap_option(&mut unwrapped) {
        if unwrapped == parse2(quote! { &'q str }).unwrap() {
            expr = quote! { #expr.as_ref().map(::std::convert::AsRef::as_ref)}
        } else {
            expr = quote! { #expr.as_ref() };
        }
    } else if !is_copy(&field_config.input_ty) {
        expr = quote! { &#expr };
    }

    if !matches!(field_config.default_mode, DefaultMode::None) {
        expr = quote! { ::foil::entity::Field::Set(#expr) }
    }

    expr
}

fn expand_to_input_record_entry(field_name: &Ident, field_config: &FieldConfig) -> TokenStream {
    let col_name = &field_config.col_name;
    let is_optional = !matches!(field_config.default_mode, DefaultMode::None);
    let alias = if is_optional {
        quote! { val }
    } else {
        quote! { self.#field_name }
    };

    let mut entry = quote! { values.add_col(#col_name, ::std::boxed::Box::new(#alias)); };
    if is_optional {
        entry = quote! {
            if let ::foil::entity::Field::Set(val) = self.#field_name {
                #entry
            }
        };
    }

    entry
}
