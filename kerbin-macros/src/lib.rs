use darling::{
    ast::{Data, Fields, Style},
    *,
};
use proc_macro::TokenStream;
use quote::quote;
use syn::{Ident, ItemFn, Type, parse_macro_input};

#[derive(Debug, FromMeta, Default)]
#[darling(derive_syn_parse, default)]
struct MacroArgs {}

/**
Exports the provided function to the Plugin System by marking it as nomangle, this allows for a consistent system
*/
#[proc_macro_attribute]
pub fn kerbin(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let _args: MacroArgs = match syn::parse(args) {
        Ok(v) => v,
        Err(e) => {
            return e.to_compile_error().into();
        }
    };

    let item = parse_macro_input!(input as ItemFn);

    let vis = item.vis;

    let mut sig = item.sig;
    let mut attr_tokens = quote! {};
    let async_ = match sig.asyncness.is_some() {
        true => {
            attr_tokens = quote! { #[async_ffi::async_ffi] };
            quote! { async }
        }
        false => quote! {},
    };

    sig.asyncness = None;

    let body = item.block;

    let output = quote! {
        #[unsafe(no_mangle)]
        #attr_tokens
        #vis #async_ extern "C" #sig {
            #body
        }
    };

    output.into()
}

#[derive(FromDeriveInput, Debug)]
#[darling(attributes(command))]
struct CommandInfo {
    ident: Ident,
    data: Data<CommandVariant, CommandField>,
    #[darling(default)]
    rename_all: Option<String>,
}

#[derive(FromVariant, Debug)]
#[darling(attributes(command))]
struct CommandVariant {
    ident: Ident,
    fields: Fields<CommandField>,
    name: Option<String>,
}

#[derive(FromField, Debug)]
#[darling(attributes(command))]
struct CommandField {
    ident: Option<Ident>,
    ty: Type,
    #[darling(default)]
    type_name: Option<String>,
    #[darling(default)]
    name: Option<String>,
}

#[proc_macro_derive(Command, attributes(command))]
pub fn command_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    let info = CommandInfo::from_derive_input(&ast).unwrap();

    let variants = match info.data {
        Data::Enum(variants) => variants,
        _ => panic!("Command can only be derived on enums."),
    };

    let rename_all = info.rename_all;

    // Generate the AsCommandInfo implementation
    let info_matches = variants
        .iter()
        .map(|variant| {
            let ident = &variant.ident;
            let variant_name = if let Some(ref name) = variant.name {
                name.clone()
            } else if let Some(ref rename_style) = rename_all {
                match rename_style.as_str() {
                    "snake_case" => to_snake_case(&ident.to_string()),
                    _ => ident.to_string(),
                }
            } else {
                ident.to_string()
            };

            let field_name_types = variant
                .fields
                .iter()
                .map(|field| {
                    let name = if let Some(ref field_ident) = field.ident {
                        field
                            .name
                            .clone()
                            .unwrap_or_else(|| field_ident.to_string())
                    } else {
                        field.name.clone().unwrap_or_else(|| "_".to_string())
                    };

                    let field_ty = &field.ty;
                    let type_name = field
                        .type_name
                        .clone()
                        .unwrap_or(quote! { #field_ty }.to_string());

                    quote! { (#name.to_string(), #type_name.to_string()) }
                })
                .collect::<Vec<_>>();

            quote! {
                CommandInfo {
                    name: #variant_name.to_string(),
                    args: vec![#(#field_name_types),*],
                }
            }
        })
        .collect::<Vec<_>>();

    let as_command_info_impl = {
        let ident = &info.ident;
        quote! {
            impl AsCommandInfo for #ident {
                fn infos() -> Vec<CommandInfo> {
                    vec![
                        #(#info_matches),*
                    ]
                }
            }
        }
    };

    // Generate the CommandFromStr implementation
    let match_arms = variants
        .iter()
        .map(|variant| {
            let ident = &variant.ident;
            let variant_name = if let Some(ref name) = variant.name {
                name.clone()
            } else if let Some(ref rename_style) = rename_all {
                match rename_style.as_str() {
                    "snake_case" => to_snake_case(&ident.to_string()),
                    _ => ident.to_string(),
                }
            } else {
                ident.to_string()
            };

            let num_required_args = variant
                .fields
                .iter()
                .filter(|f| get_option_inner_type(&f.ty).is_none())
                .count();
            let num_total_args = variant.fields.len();

            let arg_check = if num_required_args == num_total_args {
                quote! {
                    if val.len() != #num_total_args + 1 {
                        return None;
                    }
                }
            } else {
                quote! {
                    if val.len() < #num_required_args + 1 || val.len() > #num_total_args + 1 {
                        return None;
                    }
                }
            };

            let mut arg_idx = 1;
            let field_parsers_and_names = variant
                .fields
                .iter()
                .map(|field| {
                    let ty = &field.ty;
                    let var = syn::Ident::new(
                        &format!("arg_{}", arg_idx),
                        proc_macro2::Span::call_site(),
                    );
                    let idx_usize = arg_idx as usize;

                    let parser = if let Some(inner_ty) = get_option_inner_type(ty) {
                        quote! {
                            let #var = if let Some(s) = val.get(#idx_usize) {
                                Some(match s.parse::<#inner_ty>() {
                                    Ok(t) => t,
                                    Err(e) => return Some(Err(e.to_string())),
                                })
                            } else {
                                None
                            };
                        }
                    } else {
                        quote! {
                            let #var = match val.get(#idx_usize) {
                                Some(s) => match s.parse::<#ty>() {
                                    Ok(t) => t,
                                    Err(e) => return Some(Err(e.to_string())),
                                },
                                None => return None,
                            };
                        }
                    };

                    let field_name_assignment = match variant.fields.style {
                        Style::Struct => {
                            let field_ident = field.ident.as_ref().unwrap();
                            quote! { #field_ident: #var }
                        }
                        Style::Tuple => quote! { #var },
                        Style::Unit => quote! {},
                    };

                    arg_idx += 1;
                    (parser, field_name_assignment)
                })
                .collect::<Vec<_>>();

            let field_parsers = field_parsers_and_names.iter().map(|(p, _)| p);
            let field_names = field_parsers_and_names.iter().map(|(_, n)| n);

            let fields = match variant.fields.style {
                Style::Struct => quote! { { #(#field_names),* } },
                Style::Tuple => quote! { ( #(#field_names),* ) },
                Style::Unit => quote! {},
            };

            quote! {
                #variant_name => {
                    #arg_check
                    #(#field_parsers)*
                    Some(Ok(Box::new(Self::#ident #fields)))
                }
            }
        })
        .collect::<Vec<_>>();

    let command_from_str_impl = {
        let ident = &info.ident;
        quote! {
            impl CommandFromStr for #ident {
                fn from_str(val: &[String]) -> Option<Result<Box<dyn Command>, String>> {
                    match val.get(0).map(|s| s.as_str())? {
                        #(#match_arms),*
                        _ => None,
                    }
                }
            }
        }
    };

    let expanded = quote! {
        #as_command_info_impl
        #command_from_str_impl
    };

    expanded.into()
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_uppercase() {
            if !result.is_empty() {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

fn get_option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}
