use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, DeriveInput, Type};

pub fn derive_delete(input: &DeriveInput) -> TokenStream {
    let dbs = dbs!();
    let entity_ident = &input.ident;

    quote! {
        #(
            #[automatically_derived]
            impl ::foil::entity::Delete<#dbs> for #entity_ident {}
        )*
    }
}
