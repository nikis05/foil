use std::{collections::HashMap, str::FromStr};

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse2, spanned::Spanned, Data, DataStruct, DeriveInput, Error, Fields, Ident, Lit, LitStr,
    Result, Type,
};

use crate::{
    attrs::Attrs,
    types::{contains_q_lifetime, into_input_type, is_copy, unwrap_option},
};

pub fn derive_update(input: DeriveInput) -> Result<TokenStream> {
    let dbs = [parse2(TokenStream::from_str("::sqlx::Postgres").unwrap()).unwrap()];
    let config = extract_config(input)?;

    let update = dbs
        .iter()
        .map(|db| expand_update(db, &config))
        .collect::<TokenStream>();
    let patch = expand_patch(&dbs, &config);

    let setters = expand_setters(&config);

    Ok(quote! {
        #update
        #patch
        #setters
    })
}

struct Config {
    entity_ident: Ident,
    patch_ident: Ident,
    fields: HashMap<Ident, FieldConfig>,
    generate_setters: bool,
}

struct FieldConfig {
    col_name: LitStr,
    input_ty: Type,
}

fn extract_config(input: DeriveInput) -> Result<Config> {
    let input_span = input.span();
    let entity_ident = input.ident;
    let patch_ident = Ident::new(&format!("{}Patch", entity_ident), Span::call_site());
    let mut fields = HashMap::new();

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

            let config = extract_field_config(&name, &ty, attrs)?;

            fields.insert(name, config);
        }
    }

    let generate_setters = if let Some(lit) = attrs.get_name_value("setters")? {
        if let Lit::Bool(lit_bool) = lit {
            lit_bool.value
        } else {
            return Err(Error::new(lit.span(), "expected bool literal"));
        }
    } else {
        true
    };

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

            let config = extract_field_config(&name, &ty, attrs)?;

            fields.insert(name, config);
        }

        Ok(Config {
            entity_ident,
            patch_ident,
            fields,
            generate_setters,
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

    Ok(FieldConfig { col_name, input_ty })
}

fn expand_update(db: &Type, config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let patch_ident = &config.patch_ident;
    let field_names = config.fields.keys();
    let field_exprs = config.fields.iter().map(|(_, field_config)| {
        if is_copy(&field_config.input_ty) {
            quote! { val }
        } else if unwrap_option(&mut field_config.input_ty.clone()) {
            quote! { val.map(::std::borrow::ToOwned::to_owned) }
        } else {
            quote!(::std::borrow::ToOwned::to_owned(val))
        }
    });

    quote! {
        #[automatically_derived]
        impl ::foil::entity::Update<#db> for #entity_ident {
            type Patch<'q> = #patch_ident<'q>;

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
    let field_names = config.fields.keys().collect::<Vec<_>>();
    let col_names = config
        .fields
        .iter()
        .map(|(_, field_config)| &field_config.col_name)
        .collect::<Vec<_>>();
    let field_input_types = config
        .fields
        .iter()
        .map(|(_, field_config)| &field_config.input_ty)
        .collect::<Vec<_>>();
    let to_input_record_impls = dbs
        .iter()
        .map(|db| {
            quote! {
                #[automatically_derived]
                impl<'q> ::foil::manager::ToInputRecord<'q, #db> for #patch_ident<'q> {
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
        struct #patch_ident<'q> {
            #(
                #field_names: ::foil::entity::Field<#field_input_types>
            ),*
        }

        #to_input_record_impls
    }
}

fn expand_setters(config: &Config) -> TokenStream {
    if !config.generate_setters {
        return TokenStream::new();
    }

    let entity_ident = &config.entity_ident;
    let patch_ident = &config.patch_ident;

    let setters = config
        .fields
        .iter()
        .map(|(field_name, field_config)| {
            let setter_name = Ident::new(&format!("set_{}", field_name), Span::call_site());
            let input_ty = &field_config.input_ty;
            let q_lifetime = if contains_q_lifetime(input_ty) {
                quote! { 'q: 'o }
            } else {
                TokenStream::new()
            };
            let field_names = config.fields.keys();
            let field_exprs = config.fields.keys().map(|other_field_name| {
                if other_field_name == field_name {
                    quote! { ::foil::entity::Field::Set(#field_name) }
                } else {
                    quote! { ::foil::entity::Field::Omit }
                }
            });

            quote! {
                fn #setter_name<
                    'm: 'o,
                    //'q: 'o,
                    #q_lifetime
                    'e: 'o,
                    'o,
                    M: ::foil::manager::Manager<'m, DB>,
                    DB: ::sqlx::Database,
                >(
                    &'e mut self,
                    manager: M,
                    #field_name: #input_ty,
                ) -> ::foil::manager::BoxFuture<'o, Result<(), M::Error>>
                where
                    Self: ::foil::entity::Update<DB, Patch<'q> = #patch_ident<'q>>,
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
        })
        .collect::<TokenStream>();

    quote! {
        #[automatically_derived]
        impl #entity_ident {
            #setters
        }
    }
}
