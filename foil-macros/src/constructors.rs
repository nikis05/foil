use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse2,
    punctuated::Punctuated,
    token::{Brace, Paren},
    Error, Expr, Ident, Result, Token,
};

pub struct SelectorInput {
    ident: Ident,
    #[allow(dead_code)]
    brace: Brace,
    fields: Punctuated<SelectorField, Token![,]>,
}

impl Parse for SelectorInput {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let content;

        Ok(Self {
            ident: input.parse()?,
            brace: braced!(content in input),
            fields: content.parse_terminated(SelectorField::parse)?,
        })
    }
}

struct SelectorField {
    name: Ident,
    value: SelectorFieldValue,
}

impl Parse for SelectorField {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
            value: input.parse()?,
        })
    }
}

enum SelectorFieldValue {
    Shorthand,
    Expr {
        #[allow(dead_code)]
        colon: Token![:],
        find_operator: Box<FindOperator>,
    },
}

impl Parse for SelectorFieldValue {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(if input.peek(Token![:]) {
            Self::Expr {
                colon: input.parse()?,
                find_operator: input.parse()?,
            }
        } else {
            Self::Shorthand
        })
    }
}

#[derive(Clone)]
enum FindOperator {
    Eq(EqOperator),
    Ne(NeOperator),
    In(InOperator),
    NotIn(NotInOperator),
}

impl Parse for FindOperator {
    fn parse(input: ParseStream) -> Result<Self> {
        if let Some((ident, _)) = input.cursor().ident() {
            if ident == "NE" {
                input.parse().map(Self::Ne)
            } else if ident == "IN" {
                input.parse().map(Self::In)
            } else if ident == "NOT_IN" {
                input.parse().map(Self::NotIn)
            } else {
                input.parse().map(Self::Eq)
            }
        } else {
            input.parse().map(Self::Eq)
        }
    }
}

#[derive(Clone)]
struct EqOperator {
    value: Expr,
}

impl Parse for EqOperator {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            value: input.parse()?,
        })
    }
}

#[derive(Clone)]
struct NeOperator {
    #[allow(dead_code)]
    ident: Ident,
    #[allow(dead_code)]
    paren: Paren,
    value: Expr,
}

impl Parse for NeOperator {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        if ident != "NE" {
            return Err(Error::new(ident.span(), "expected NE"));
        }
        let content;
        Ok(Self {
            ident,
            paren: parenthesized!(content in input),
            value: content.parse()?,
        })
    }
}

#[derive(Clone)]
struct InOperator {
    #[allow(dead_code)]
    ident: Ident,
    #[allow(dead_code)]
    paren: Paren,
    values: Punctuated<Expr, Token![,]>,
}

impl Parse for InOperator {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        if ident != "IN" {
            return Err(Error::new(ident.span(), "expected IN"));
        }
        let content;
        Ok(Self {
            ident,
            paren: parenthesized!(content in input),
            values: content.parse_terminated(Expr::parse)?,
        })
    }
}

#[derive(Clone)]
struct NotInOperator {
    #[allow(dead_code)]
    ident: Ident,
    #[allow(dead_code)]
    paren: Paren,
    values: Punctuated<Expr, Token![,]>,
}

impl Parse for NotInOperator {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        if ident != "NOT_IN" {
            return Err(Error::new(ident.span(), "expected NOT_IN"));
        }
        let content;
        Ok(Self {
            ident,
            paren: parenthesized!(content in input),
            values: content.parse_terminated(Expr::parse)?,
        })
    }
}

pub fn expand_selector(input: SelectorInput) -> TokenStream {
    let selector_ident = input.ident;
    let field_names = input.fields.iter().map(|field| &field.name);
    let field_values = input.fields.iter().map(|field| {
        let field_name = &field.name;
        let find_operator = if let SelectorFieldValue::Expr {
            colon: _,
            find_operator,
        } = &field.value
        {
            *find_operator.clone()
        } else {
            FindOperator::Eq(EqOperator {
                value: parse2(quote! { #field_name }).unwrap(),
            })
        };
        match find_operator {
            FindOperator::Eq(eq_operator) => {
                let value = &eq_operator.value;
                quote! { ::foil::manager::FindOperator::Eq(#value) }
            }
            FindOperator::Ne(ne_operator) => {
                let value = &ne_operator.value;
                quote! { ::foil::manager::FindOperator::Ne(#value) }
            }
            FindOperator::In(in_operator) => {
                let values = in_operator.values.iter();
                quote! {
                    ::foil::manager::FindOperator::In(::std::vec![
                        #(
                            #values
                        ),*
                    ])
                }
            }
            FindOperator::NotIn(not_in_operator) => {
                let values = not_in_operator.values.iter();
                quote! {
                    ::foil::manager::FindOperator::NotIn(::std::vec![
                        #(
                            #values
                        ),*
                    ])
                }
            }
        }
    });

    quote! {
        #selector_ident {
            #(
                #field_names: ::foil::entity::Field::Set(#field_values),
            )*
            ..::std::default::Default::default()
        }
    }
}

pub struct PatchInput {
    ident: Ident,
    #[allow(dead_code)]
    brace: Brace,
    fields: Punctuated<PatchField, Token![,]>,
}

impl Parse for PatchInput {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let content;

        Ok(Self {
            ident: input.parse()?,
            brace: braced!(content in input),
            fields: content.parse_terminated(PatchField::parse)?,
        })
    }
}

struct PatchField {
    name: Ident,
    value: PatchFieldValue,
}

impl Parse for PatchField {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
            value: input.parse()?,
        })
    }
}

enum PatchFieldValue {
    Shorthand,
    Expr {
        #[allow(dead_code)]
        colon: Token![:],
        expr: Box<Expr>,
    },
}

impl Parse for PatchFieldValue {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(if input.peek(Token![:]) {
            Self::Expr {
                colon: input.parse()?,
                expr: input.parse()?,
            }
        } else {
            Self::Shorthand
        })
    }
}

pub fn expand_patch(input: PatchInput, opt: bool) -> TokenStream {
    let patch_ident = input.ident;
    let field_names = input.fields.iter().map(|field| &field.name);
    let field_values = input.fields.iter().map(|field| {
        if let PatchFieldValue::Expr { colon: _, expr } = &field.value {
            quote! { #expr }
        } else {
            let field_name = &field.name;
            quote! { #field_name }
        }
    });

    if opt {
        quote! {
            #patch_ident {
                #(
                    #field_names: if let ::std::option::Option::Some(val) = #field_values {
                        ::foil::entity::Field::Set(val)
                    } else {
                        ::foil::entity::Field::Omit
                    },
                )*
                ..::std::default::Default::default()
            }
        }
    } else {
        quote! {
            #patch_ident {
                #(
                    #field_names: ::foil::entity::Field::Set(#field_values),
                )*
                ..::std::default::Default::default()
            }
        }
    }
}
