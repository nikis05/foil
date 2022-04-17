#![allow(unstable_name_collisions)]
#![warn(clippy::pedantic)]
#![forbid(unused_must_use)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

use crate::entity::derive_entity;
use constructors::{expand_input, expand_selector, InputInput, SelectorInput};
use create::derive_create;
use delete::derive_delete;
use syn::{parse_macro_input, DeriveInput};
use update::derive_update;

macro_rules! dbs {
    () => {
        [
            #[cfg(feature = "mysql")]
            quote! { ::sqlx::MySql },
            #[cfg(feature = "mssql")]
            quote! { ::sqlx::Mssql },
            #[cfg(feature = "postgres")]
            quote! { ::sqlx::Postgres },
            #[cfg(feature = "sqlite")]
            quote! { ::sqlx::Sqlite },
            #[cfg(feature = "any")]
            quote! { ::sqlx::Any },
        ]
        .into_iter()
        .map(|db| parse2(db).unwrap())
        .collect::<Vec<Type>>()
    };
}

mod attrs;
mod constructors;
mod create;
mod delete;
mod entity;
mod relations;
mod types;
mod update;

#[proc_macro_derive(Entity, attributes(foil))]
pub fn entity(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_entity(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Create, attributes(foil))]
pub fn create(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_create(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Update, attributes(foil))]
pub fn update(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_update(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Delete)]
pub fn delete(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_delete(&input).into()
}

#[proc_macro]
pub fn selector(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as SelectorInput);
    expand_selector(input).into()
}

#[proc_macro]
pub fn input(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as InputInput);
    expand_input(input).into()
}
