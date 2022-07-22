use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse2,
    punctuated::Punctuated,
    token::{Brace, Paren},
    Error, Expr, Ident, Result, Token, Type,
};

pub struct SelectorInput {
    ty: Type,
    #[allow(dead_code)]
    brace: Brace,
    fields: Punctuated<SelectorField, Token![,]>,
}

impl Parse for SelectorInput {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let content;

        Ok(Self {
            ty: input.parse()?,
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
    values: InValues,
}

impl Parse for InOperator {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        if ident != "IN" {
            return Err(Error::new(ident.span(), "expected IN"));
        }
        Ok(Self {
            ident,
            values: input.parse()?,
        })
    }
}

#[derive(Clone)]
struct NotInOperator {
    #[allow(dead_code)]
    ident: Ident,
    values: InValues,
}

impl Parse for NotInOperator {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        if ident != "NOT_IN" {
            return Err(Error::new(ident.span(), "expected NOT_IN"));
        }
        Ok(Self {
            ident,
            values: input.parse()?,
        })
    }
}

#[derive(Clone)]
enum InValues {
    List {
        #[allow(dead_code)]
        paren: Paren,
        values: Punctuated<Expr, Token![,]>,
    },
    Vec {
        #[allow(dead_code)]
        paren: Paren,
        #[allow(dead_code)]
        spread_token: Token![..],
        expr: Box<Expr>,
    },
}

impl Parse for InValues {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        let paren = parenthesized!(content in input);
        Ok(if content.peek(Token![..]) {
            Self::Vec {
                paren,
                spread_token: content.parse()?,
                expr: content.parse()?,
            }
        } else {
            Self::List {
                paren,
                values: content.parse_terminated(Expr::parse)?,
            }
        })
    }
}

pub fn expand_selector(input: SelectorInput) -> TokenStream {
    let selector_ty = input.ty;
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
            FindOperator::In(in_operator) => match in_operator.values {
                InValues::List { paren: _, values } => {
                    let values = values.iter();
                    quote! {
                        ::foil::manager::FindOperator::In(::std::vec![
                            #(
                                #values
                            ),*
                        ])
                    }
                }
                InValues::Vec {
                    paren: _,
                    spread_token: _,
                    expr,
                } => {
                    quote! {
                        ::foil::manager::FindOperator::In(#expr)
                    }
                }
            },
            FindOperator::NotIn(not_in_operator) => match not_in_operator.values {
                InValues::List { paren: _, values } => {
                    let values = values.iter();
                    quote! {
                        ::foil::manager::FindOperator::In(::std::vec![
                            #(
                                #values
                            ),*
                        ])
                    }
                }
                InValues::Vec {
                    paren: _,
                    spread_token: _,
                    expr,
                } => {
                    quote! {
                        ::foil::manager::FindOperator::In(#expr)
                    }
                }
            },
        }
    });

    quote! {
        #selector_ty {
            #(
                #field_names: ::foil::entity::Field::Set(#field_values),
            )*
            ..::std::default::Default::default()
        }
    }
}

pub struct PatchInput {
    ty: Type,
    #[allow(dead_code)]
    brace: Brace,
    fields: Punctuated<PatchField, Token![,]>,
}

impl Parse for PatchInput {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let content;

        Ok(Self {
            ty: input.parse()?,
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
    let patch_ty = input.ty;
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
            #patch_ty {
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
            #patch_ty {
                #(
                    #field_names: ::foil::entity::Field::Set(#field_values),
                )*
                ..::std::default::Default::default()
            }
        }
    }
}
