use crate::{
    attrs::Attrs,
    types::{contains_q_lifetime, into_input_type, unwrap_option},
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::str::FromStr;
use syn::{
    parse2, spanned::Spanned, Data, DataStruct, DeriveInput, Error, Fields, Ident, Lit, LitStr,
    Result, Type, Visibility,
};

pub fn derive_update(input: DeriveInput) -> Result<TokenStream> {
    let dbs = dbs!();
    let config = extract_config(input)?;

    let update = dbs
        .iter()
        .map(|db| expand_update(db, &config))
        .collect::<TokenStream>();
    let patch = expand_patch(&dbs, &config);

    let setters = dbs
        .iter()
        .map(|db| expand_setters(db, &config))
        .collect::<TokenStream>();

    Ok(quote! {
        #update
        #patch
        #setters
    })
}

struct Config {
    entity_ident: Ident,
    vis: Visibility,
    patch_ident: Ident,
    patch_is_generic: bool,
    fields: Vec<FieldConfig>,
}

struct FieldConfig {
    name: Ident,
    col_name: LitStr,
    input_ty: Type,
    ty: Type,
}

fn extract_config(input: DeriveInput) -> Result<Config> {
    let input_span = input.span();
    let entity_ident = input.ident;
    let vis = input.vis;
    let patch_ident = Ident::new(&format!("{}Patch", entity_ident), Span::call_site());
    let mut fields = Vec::new();

    let mut attrs = Attrs::extract(input.attrs)?;

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

    attrs.ignore(&["table", "id_field"]);
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

        let patch_is_generic = fields
            .iter()
            .any(|field_config| contains_q_lifetime(&field_config.input_ty));

        Ok(Config {
            entity_ident,
            vis,
            patch_ident,
            patch_is_generic,
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

    Ok(FieldConfig {
        name,
        col_name,
        input_ty,
        ty,
    })
}

fn expand_update(db: &Type, config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let patch_ident = &config.patch_ident;
    let patch_type = if config.patch_is_generic {
        quote! { #patch_ident<'q> }
    } else {
        quote! { #patch_ident }
    };
    let field_names = config.fields.iter().map(|field_config| &field_config.name);
    let field_exprs = config.fields.iter().map(|field_config| {
        if field_config.ty == field_config.input_ty {
            quote! { val }
        } else {
            let mut unwrapped_input_ty = field_config.input_ty.clone();
            if unwrap_option(&mut unwrapped_input_ty) {
                let mut unwrapped_ty = field_config.ty.clone();
                if unwrap_option(&mut unwrapped_ty) && unwrapped_ty == unwrapped_input_ty {
                    quote! { val }
                } else {
                    quote! { val.map(::std::borrow::ToOwned::to_owned) }
                }
            } else {
                quote! { ::std::borrow::ToOwned::to_owned(val)}
            }
        }
    });

    quote! {
        #[automatically_derived]
        impl ::foil::entity::Update<#db> for #entity_ident {
            type Patch<'q> = #patch_type;

            fn apply_patch(&mut self, patch: Self::Patch<'_>) {
                #(
                    if let ::foil::entity::Field::Set(val) = patch.#field_names {
                        self.#field_names = #field_exprs;
                    }
                )*
            }
        }
    }
}

fn expand_patch(dbs: &[Type], config: &Config) -> TokenStream {
    let patch_ident = &config.patch_ident;
    let patch_type = if config.patch_is_generic {
        quote! { #patch_ident<'q> }
    } else {
        quote! { #patch_ident }
    };
    let vis = &config.vis;
    let field_names = config
        .fields
        .iter()
        .map(|field_config| &field_config.name)
        .collect::<Vec<_>>();
    let col_names = config
        .fields
        .iter()
        .map(|field_config| &field_config.col_name)
        .collect::<Vec<_>>();
    let field_input_types = config
        .fields
        .iter()
        .map(|field_config| &field_config.input_ty)
        .collect::<Vec<_>>();
    let to_input_record_impls = dbs
        .iter()
        .map(|db| {
            quote! {
                #[automatically_derived]
                impl<'q> ::foil::manager::ToInputRecord<'q, #db> for #patch_type {
                    fn to_input_record(&self) -> ::foil::manager::InputRecord<'q, #db> {
                        let mut patch = ::foil::manager::InputRecord::new();
                        #(
                            if let ::foil::entity::Field::Set(val) = self.#field_names {
                                patch.add_col(#col_names, ::std::boxed::Box::new(val));
                            }
                        )*
                        patch
                    }
                }
            }
        })
        .collect::<TokenStream>();

    quote! {
        #[derive(::std::default::Default)]
        #vis struct #patch_type {
            #(
                pub #field_names: ::foil::entity::Field<#field_input_types>
            ),*
        }

        #to_input_record_impls
    }
}

fn expand_setters(db: &Type, config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let vis = &config.vis;
    let setters_ident = Ident::new(&format!("{}Setters", entity_ident), Span::call_site());

    let setter_signatures = config.fields.iter().map(|field_config| {
        expand_setter(config, field_config, false, &parse2(quote! { DB }).unwrap())
    });
    let setters = config
        .fields
        .iter()
        .map(|field_config| expand_setter(config, field_config, true, db));

    quote! {
        #[automatically_derived]
        #vis trait #setters_ident<DB: ::sqlx::Database>: ::foil::entity::Update<DB> {
            #(
                #setter_signatures
            )*
        }

        #[automatically_derived]
        impl #setters_ident<#db> for #entity_ident {
            #(
                #setters
            )*
        }
    }
}

fn expand_setter(
    config: &Config,
    field_config: &FieldConfig,
    expand_impl: bool,
    db: &Type,
) -> TokenStream {
    let patch_ident = &config.patch_ident;
    let field_name = &field_config.name;
    let setter_name = Ident::new(&format!("set_{}", field_name), Span::call_site());
    let input_ty = &field_config.input_ty;
    let q_lifetime = if contains_q_lifetime(input_ty) {
        quote! { 'q: 'o, }
    } else {
        TokenStream::new()
    };
    let field_names = config
        .fields
        .iter()
        .map(|field_config| &field_config.name)
        .collect::<Vec<_>>();
    let field_exprs = field_names.iter().map(|other_field_name| {
        if *other_field_name == field_name {
            quote! { ::foil::entity::Field::Set(#field_name) }
        } else {
            quote! { ::foil::entity::Field::Omit }
        }
    });

    let impl_ = if expand_impl {
        quote! {
            {
                self.patch(
                    manager,
                    #patch_ident {
                        #(
                            #field_names: #field_exprs
                        ),*
                    },
                )
            }
        }
    } else {
        quote! { ; }
    };

    quote! {
        fn #setter_name<
            'm: 'o,
            #q_lifetime
            'e: 'o,
            'o,
            M: ::foil::manager::Manager<'m, #db>,
        >(
            &'e mut self,
            manager: M,
            #field_name: #input_ty,
        ) -> ::foil::manager::BoxFuture<'o, ::std::result::Result<(), M::Error>>
        #impl_
    }
}
