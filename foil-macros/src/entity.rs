use std::{collections::BTreeMap, str::FromStr};

use heck::{ToSnakeCase, ToUpperCamelCase};

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{
    parse2, spanned::Spanned, Data, DataStruct, DeriveInput, Error, Fields, Lit, LitStr, Result,
    Type, Visibility,
};

use crate::{attrs::Attrs, types::into_input_type};

pub fn derive_entity(input: DeriveInput) -> Result<TokenStream> {
    let dbs = [parse2(TokenStream::from_str("::sqlx::Postgres").unwrap()).unwrap()];
    let config = extract_config(input)?;

    let entity = dbs
        .iter()
        .map(|db| expand_entity(db, &config))
        .collect::<TokenStream>();

    let from_record = dbs
        .iter()
        .map(|db| expand_from_record(db, &config))
        .collect::<TokenStream>();

    let col = expand_col(&config);

    let selector = expand_selector(&dbs, &config);

    let lazy_columns = expand_lazy_columns(&config);

    Ok(quote! {
        #entity
        #from_record
        #col
        #selector
        #lazy_columns
    })
}

struct Config {
    entity_ident: Ident,
    vis: Visibility,
    col_ident: Ident,
    selector_ident: Ident,
    table_name: LitStr,
    id_field_name: Ident,
    fields: BTreeMap<Ident, FieldConfig>,
}

struct FieldConfig {
    col_name: LitStr,
    ty: Type,
    input_ty: Type,
    is_lazy: bool,
}

fn extract_config(input: DeriveInput) -> Result<Config> {
    let input_span = input.span();
    let entity_ident = input.ident;
    let vis = input.vis;

    let mut fields = BTreeMap::new();

    let mut attrs = Attrs::extract(input.attrs)?;

    let col_ident = Ident::new(&format!("{}Col", entity_ident), Span::call_site());

    let selector_ident = Ident::new(&format!("{}Selector", entity_ident), Span::call_site());

    let table_name = attrs
        .get_name_value("table")?
        .map(|lit| {
            if let Lit::Str(lit_str) = lit {
                Ok(lit_str)
            } else {
                Err(Error::new(lit.span(), "expected string literal"))
            }
        })
        .transpose()?
        .unwrap_or_else(|| {
            LitStr::new(&entity_ident.to_string().to_snake_case(), Span::call_site())
        });

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

            let config = extract_field_config(&name, ty, attrs, true)?;

            fields.insert(name, config);
        }
    }

    attrs.ignore(&["setters"]);
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
            let config = extract_field_config(&name, ty, attrs, false)?;

            fields.insert(name, config);
        }

        if !fields.contains_key(&id_field_name) {
            return Err(Error::new(
                id_field_name_span.unwrap_or(input_span),
                &format!("field {} does not exist", id_field_name),
            ));
        }

        Ok(Config {
            entity_ident,
            vis,
            col_ident,
            selector_ident,
            table_name,
            id_field_name,
            fields,
        })
    } else {
        Err(Error::new(input_span, "expected struct with named fields"))
    }
}

fn extract_field_config(
    name: &Ident,
    ty: Type,
    mut attrs: Attrs,
    is_lazy: bool,
) -> Result<FieldConfig> {
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

    attrs.ignore(&["generated", "default", "default_with"]);
    attrs.done()?;

    Ok(FieldConfig {
        col_name,
        ty,
        input_ty,
        is_lazy,
    })
}

fn expand_entity(db: &Type, config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let selector_ident = &config.selector_ident;
    let col_ident = &config.col_ident;
    let table_name = &config.table_name;
    let id_field_name = &config.id_field_name;
    let id_field = config.fields.get(id_field_name).unwrap();
    let id_type = &id_field.ty;
    let id_col_name = &id_field.col_name;
    let col_names = config.fields.iter().filter_map(|(_, field_config)| {
        if field_config.is_lazy {
            None
        } else {
            Some(&field_config.col_name)
        }
    });

    quote! {
        #[automatically_derived]
        impl ::foil::entity::Entity<#db> for #entity_ident {
            type Col = #col_ident;
            type Id = #id_type;
            type Selector<'q> = #selector_ident<'q>;

            fn table_name() -> &'static str {
                #table_name
            }

            fn col_names() -> &'static [&'static str] {
                &[
                    #(
                        #col_names
                    ),*
                ]
            }

            fn id_col_name() -> &'static str {
                #id_col_name
            }

            fn id(&self) -> Self::Id {
                self.#id_field_name
            }
        }
    }
}

fn expand_from_record(db: &Type, config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let field_names = config.fields.keys();
    let col_names = config.fields.iter().filter_map(|(_, field_config)| {
        if field_config.is_lazy {
            None
        } else {
            Some(&field_config.col_name)
        }
    });

    quote! {
        #[automatically_derived]
        impl ::foil::manager::FromRecord<#db> for #entity_ident {
            fn from_record(record: &::foil::manager::Record<#db>) -> ::std::result::Result<Self, ::foil::manager::RecordError> {
                ::std::result::Result::Ok(#entity_ident {
                    #(
                        #field_names: record.col(#col_names)?
                    ),*
                })
            }
        }
    }
}

fn expand_col(config: &Config) -> TokenStream {
    let vis = &config.vis;
    let col_ident = &config.col_ident;

    let col_names = config
        .fields
        .iter()
        .map(|(_, field_config)| &field_config.col_name);

    let variants = col_names
        .clone()
        .map(|col_name| Ident::new(&col_name.value().to_upper_camel_case(), Span::call_site()))
        .collect::<Vec<_>>();

    quote! {
        #[derive(::std::clone::Clone, ::std::marker::Copy)]
        #vis enum #col_ident {
            #(
                #variants
            ),*
        }

        #[automatically_derived]
        impl ::foil::entity::Col for #col_ident {
            fn as_str(&self) -> &'static str {
                match self {
                    #(
                        Self::#variants => #col_names
                    ),*
                }
            }
        }
    }
}

fn expand_selector(dbs: &[Type], config: &Config) -> TokenStream {
    let vis = &config.vis;
    let selector_ident = &config.selector_ident;
    let field_names = config.fields.keys().collect::<Vec<_>>();
    let selector_field_types = config
        .fields
        .iter()
        .map(|(_, field_config)| &field_config.input_ty)
        .collect::<Vec<_>>();
    let col_names = config
        .fields
        .iter()
        .map(|(_, field_config)| &field_config.col_name)
        .collect::<Vec<_>>();

    let into_selector_impls = dbs
        .iter()
        .map(|db| {
            quote! {
                #[automatically_derived]
                impl<'q> ::foil::manager::IntoSelector<'q, #db> for #selector_ident<'q> {
                    fn into_selector(self) -> ::foil::manager::Selector<'q, #db> {
                        let mut selector = ::foil::manager::Selector::new();

                        #(
                            if let ::foil::entity::Field::Set(op) = self.#field_names {
                                selector.add_col(#col_names, op.boxed());
                            }
                        )*

                        selector
                    }
                }
            }
        })
        .collect::<TokenStream>();

    quote! {
        #[derive(::std::default::Default)]
        #vis struct #selector_ident<'q> {
            #(
                #field_names: ::foil::entity::Field<::foil::manager::FindOperator<#selector_field_types>>
            ),*
        }

        #into_selector_impls
    }
}

fn expand_lazy_columns(config: &Config) -> TokenStream {
    let entity_ident = &config.entity_ident;
    let lazy_fields = config
        .fields
        .iter()
        .filter(|(_, field_config)| field_config.is_lazy)
        .collect::<Vec<_>>();
    let field_names = lazy_fields.iter().map(|(field_name, _)| field_name);
    let field_types = lazy_fields.iter().map(|(_, field_config)| &field_config.ty);
    let col_names = lazy_fields
        .iter()
        .map(|(_, field_config)| &field_config.col_name);

    quote! {
        #[automatically_derived]
        impl #entity_ident {
            #(
                pub async fn #field_names<
                    'm,
                    M: ::foil::manager::Manager<'m, DB, Error = E>,
                    E: ::std::error::Error + ::std::marker::Send + ::std::marker::Sync + 'static,
                    DB: ::sqlx::Database,
                >(
                    &self,
                    manager: M,
                ) -> ::std::result::Result<#field_types, ::foil::entity::SelectOneError<E>>
                where
                    Self: ::foil::entity::Entity<DB>,
                    str: ::sqlx::ColumnIndex<<DB as ::sqlx::Database>::Row>,
                    #field_types: ::sqlx::Type<DB> + for<'d> ::sqlx::Decode<'d, DB>,
                {
                    let mut selector = ::foil::manager::Selector::new();
                    selector.add_col("id", ::foil::manager::FindOperator::Eq(Box::new(self.id())));
                    let query = ::foil::manager::SelectQuery::<DB> {
                        table_name: <Self as ::foil::entity::Entity<DB>>::table_name(),
                        col_names: &[#col_names],
                        selectors: vec![selector],
                        order_by: None,
                        offset: None,
                        limit: None,
                    };

                    let record = manager
                        .select(query)
                        .try_next()
                        .await
                        .map_err(::foil::entity::SelectError::Manager)?
                        .ok_or(::foil::entity::SelectOneError::RowNotFound)?;

                    let value = record
                        .col(#col_names)
                        .map_err(::foil::entity::SelectError::Record)?;

                    Ok(value)
                }
            )*
        }
    }
}
