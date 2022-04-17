use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
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
    #[allow(dead_code)]
    colon: Token![:],
    find_operator: FindOperator,
}

impl Parse for SelectorField {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
            colon: input.parse()?,
            find_operator: input.parse()?,
        })
    }
}

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
    let field_values = input.fields.iter().map(|field| match &field.find_operator {
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
    });

    quote! {
        #selector_ident {
            ..::std::default::Default(),
            #(
                #field_names: ::foil::entity::Field::Set(#field_values)
            )*
        }
    }
}

pub struct InputInput {
    ident: Ident,
    #[allow(dead_code)]
    brace: Brace,
    fields: Punctuated<InputField, Token![,]>,
}

impl Parse for InputInput {
    #[allow(clippy::eval_order_dependence)]
    fn parse(input: ParseStream) -> Result<Self> {
        let content;

        Ok(Self {
            ident: input.parse()?,
            brace: braced!(content in input),
            fields: content.parse_terminated(InputField::parse)?,
        })
    }
}

struct InputField {
    name: Ident,
    #[allow(dead_code)]
    colon: Token![:],
    expr: FieldExpr,
}

impl Parse for InputField {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
            colon: input.parse()?,
            expr: input.parse()?,
        })
    }
}

enum FieldExpr {
    Set(Box<Expr>),
    Omit(Token![_]),
}

impl Parse for FieldExpr {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(if input.peek(Token![_]) {
            Self::Omit(input.parse()?)
        } else {
            Self::Set(Box::new(input.parse()?))
        })
    }
}

pub fn expand_input(input: InputInput) -> TokenStream {
    let input_ident = input.ident;
    let field_names = input.fields.iter().map(|input_field| &input_field.name);
    let field_exprs = input.fields.iter().map(|input_field| {
        if let FieldExpr::Set(expr) = &input_field.expr {
            quote! { ::foil::entity::Field::Set(#expr)}
        } else {
            quote! { ::foil::entity::Field::Omit }
        }
    });

    quote! {
        #input_ident {
            #(
                #field_names: #field_exprs
            ),*
        }
    }
}
