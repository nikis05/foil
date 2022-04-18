use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

pub fn derive_value(input: &DeriveInput) -> TokenStream {
    let entity_ident = &input.ident;

    quote! {
        #[automatically_derived]
        impl<'q, DB: ::sqlx::Database> ::foil::manager::Value<'q, DB> for #entity_ident
        where
            #entity_ident: ::sqlx::Type<DB> + ::sqlx::Encode<'q, DB>,
        {
            fn bind(
                self: ::std::boxed::Box<Self>,
                query: ::sqlx::Query<'q, DB, <DB as ::sqlx::HasArguments<'q>>::Arguments>,
            ) -> ::sqlx::Query<'q, DB, <DB as ::sqlx::HasArguments<'q>>::Arguments> {
                query.bind(*self)
            }

            fn is_null(&self) -> bool {
                false
            }

            #[allow(clippy::clone_on_copy)]
            fn to_owned_any(&self) -> ::std::boxed::Box<dyn ::std::any::Any> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }
        }
    }
}
