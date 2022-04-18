use quote::{quote, ToTokens};
use syn::{parse2, GenericArgument, Lifetime, PathArguments, PathSegment, Type};

pub fn into_input_type(mut ty: Type) -> Type {
    let should_wrap_option = unwrap_option(&mut ty);

    if !is_copy(&ty) {
        ty = to_borrowed_form(&ty);
    }

    if should_wrap_option {
        wrap_option(&mut ty);
    }

    ty
}

pub fn unwrap_option(ty: &mut Type) -> bool {
    if let Some(wrapped) = unwrap_generic(
        ty,
        &parse2::<PathSegment>(quote! { std }).unwrap(),
        &parse2::<PathSegment>(quote! { option }).unwrap(),
        "Option",
    ) {
        *ty = wrapped;
        true
    } else {
        false
    }
}

fn wrap_option(ty: &mut Type) {
    *ty = parse2(quote! {
        ::std::option::Option<#ty>
    })
    .unwrap();
}

static COPY_TYPES: [&str; 14] = [
    "bool",
    "u8",
    "u16",
    "u32",
    "u64",
    "i8",
    "i16",
    "i32",
    "i64",
    "f32",
    "f64",
    "Uuid",
    "uuid::Uuid",
    "::uuid::Uuid",
];

pub fn is_copy(ty: &Type) -> bool {
    COPY_TYPES.contains(&ty.to_token_stream().to_string().as_str())
}

fn to_borrowed_form(ty: &Type) -> Type {
    if *ty == parse2(quote! { String }).unwrap()
        || *ty == parse2(quote! { std::string::String }).unwrap()
        || *ty == parse2(quote! { ::std::string::String}).unwrap()
    {
        parse2(quote! { &'q str }).unwrap()
    } else if let Some(wrapped) = unwrap_generic(
        ty,
        &parse2(quote! { std }).unwrap(),
        &parse2(quote! { vec }).unwrap(),
        "Vec",
    ) {
        parse2(quote! { &'q[ #wrapped ] }).unwrap()
    } else {
        parse2(quote! { &'q #ty }).unwrap()
    }
}

fn unwrap_generic(
    ty: &Type,
    prefix1: &PathSegment,
    prefix2: &PathSegment,
    ident: &str,
) -> Option<Type> {
    fn unwrap_path_segment(path_segment: &PathSegment, ident: &str) -> Option<Type> {
        if path_segment.ident == ident {
            if let PathArguments::AngleBracketed(arguments) = &path_segment.arguments {
                if arguments.args.len() == 1 {
                    if let GenericArgument::Type(wrapped) = arguments.args.first().unwrap() {
                        return Some(wrapped.clone());
                    }
                }
            }
        }
        None
    }

    if let Type::Path(type_path) = ty {
        if type_path.qself.is_some() {
            return None;
        }

        let path = &type_path.path;

        if path.leading_colon.is_none() && path.segments.len() == 1 {
            let first = type_path.path.segments.first().unwrap();
            if let Some(wrapped) = unwrap_path_segment(first, ident) {
                return Some(wrapped);
            }
        } else if path.segments.len() == 3 {
            let mut segments = path.segments.iter();
            let first = segments.next().unwrap();
            let second = segments.next().unwrap();
            let third = segments.next().unwrap();

            if first == prefix1 && second == prefix2 {
                if let Some(wrapped) = unwrap_path_segment(third, ident) {
                    return Some(wrapped);
                }
            }
        }
    }

    None
}

pub fn contains_q_lifetime(ty: &Type) -> bool {
    match ty {
        Type::Array(array) => contains_q_lifetime(&array.elem),
        Type::Paren(ty) => contains_q_lifetime(&ty.elem),
        Type::Path(type_path) => type_path.path.segments.iter().any(|segment| {
            if let PathArguments::AngleBracketed(arguments) = &segment.arguments {
                arguments.args.iter().any(|argument| {
                    if let GenericArgument::Lifetime(life) = argument {
                        life.ident == "q"
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        }),
        Type::Reference(reference) => {
            if let Some(lifetime) = &reference.lifetime {
                if lifetime.ident == "q" {
                    return true;
                }
            }
            contains_q_lifetime(&reference.elem)
        }
        Type::Slice(slice) => contains_q_lifetime(&slice.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(contains_q_lifetime),
        _ => false,
    }
}
