use proc_macro2::TokenStream;
use quote::quote;
use std::str::FromStr;
use syn::{parse2, DeriveInput, Type};

pub fn derive_delete(input: &DeriveInput) -> TokenStream {
    let dbs = [parse2::<Type>(TokenStream::from_str("::sqlx::Postgres").unwrap()).unwrap()];
    let entity_ident = &input.ident;

    quote! {
        #(
            #[automatically_derived]
            impl ::foil::entity::Delete<#dbs> for #entity_ident {}
        )*
    }
}
