use crate::{
    attrs::Attrs,
    types::{into_input_type, is_string, unwrap_option, unwrap_vec},
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::str::FromStr;
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
    fields: Vec<FieldConfig>,
}

struct FieldConfig {
    name: Ident,
    col_name: LitStr,
    generated: bool,
    ty: Type,
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
    let mut fields = Vec::new();

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

            let config = extract_field_config(name, ty, attrs)?;

            fields.push(config);
        }
    }

    attrs.ignore(&["table"]);
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

            let config = extract_field_config(name, ty, attrs)?;

            fields.push(config);
        }

        if let Some(id_field) = fields
            .iter_mut()
            .find(|field_config| field_config.name == id_field_name)
        {
            id_field.generated = true;
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

fn extract_field_config(name: Ident, ty: Type, mut attrs: Attrs) -> Result<FieldConfig> {
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

    let input_ty = if attrs.get_path("copy")? {
        ty.clone()
    } else {
        into_input_type(ty.clone())
    };

    let generated = attrs.get_path("generated")?;

    attrs.done()?;

    Ok(FieldConfig {
        name,
        col_name,
        generated,
        ty,
        input_ty,
    })
}

fn expand_create(db: &Type, config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let input_ident = &config.input_ident;
    let generated_col_names = config.fields.iter().filter_map(|field_config| {
        if field_config.generated {
            Some(&field_config.col_name)
        } else {
            None
        }
    });
    let field_names = config.fields.iter().map(|field_config| &field_config.name);
    let construct_field_exprs = config.fields.iter().map(expand_construct_field_expr);

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

fn expand_construct_field_expr(field_config: &FieldConfig) -> TokenStream {
    let field_name = &field_config.name;
    let col_name = &field_config.col_name;
    let generated = field_config.generated;

    let alias = if generated {
        quote! { val }
    } else {
        quote! { input.#field_name }
    };

    let owned_expr = if field_config.ty == field_config.input_ty {
        quote! { #alias }
    } else if unwrap_option(&mut field_config.input_ty.clone()) {
        quote! { #alias.map(::std::borrow::ToOwned::to_owned) }
    } else {
        quote! { ::std::borrow::ToOwned::to_owned(#alias)}
    };

    if generated {
        quote! {
            if let ::foil::entity::Field::Set(val) = input.#field_name {
                #owned_expr
            } else {
                generated.col(#col_name)?
            }
        }
    } else {
        owned_expr
    }
}

fn expand_input(dbs: &[Type], config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let vis = &config.vis;
    let input_ident = &config.input_ident;
    let field_names = config
        .fields
        .iter()
        .map(|field_config| &field_config.name)
        .collect::<Vec<_>>();
    let field_input_types = config.fields.iter().map(|field_config| {
        let input_ty = &field_config.input_ty;
        if field_config.generated {
            quote! { ::foil::entity::Field<#input_ty> }
        } else {
            quote! { #input_ty }
        }
    });
    let field_from_exprs = config.fields.iter().map(expand_from_field_expr);
    let to_input_record_entries = config
        .fields
        .iter()
        .map(expand_to_input_record_entry)
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
                pub #field_names: #field_input_types
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

fn expand_from_field_expr(field_config: &FieldConfig) -> TokenStream {
    let field_name = &field_config.name;
    let mut expr = quote! { from.#field_name };

    if field_config.ty != field_config.input_ty {
        let mut unwrapped = field_config.ty.clone();
        if unwrap_option(&mut unwrapped) {
            if is_string(&unwrapped) || unwrap_vec(&unwrapped).is_some() {
                expr = quote! { #expr.as_ref().map(::std::convert::AsRef::as_ref)}
            } else {
                expr = quote! { #expr.as_ref() };
            }
        } else {
            expr = quote! { &#expr };
        }
    }

    if field_config.generated {
        expr = quote! { ::foil::entity::Field::Set(#expr) }
    }

    expr
}

fn expand_to_input_record_entry(field_config: &FieldConfig) -> TokenStream {
    let field_name = &field_config.name;
    let col_name = &field_config.col_name;
    let generated = field_config.generated;
    let alias = if generated {
        quote! { val }
    } else {
        quote! { self.#field_name }
    };

    let mut entry = quote! { values.add_col(#col_name, ::std::boxed::Box::new(#alias)); };
    if generated {
        entry = quote! {
            if let ::foil::entity::Field::Set(val) = self.#field_name {
                #entry
            }
        };
    }

    entry
}
