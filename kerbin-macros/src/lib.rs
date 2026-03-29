use darling::{
    ast::{Data, Fields, Style},
    *,
};
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Ident, Path, Type, parse_macro_input};

#[proc_macro_derive(State)]
pub fn derive_state(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl StaticState for #name {
            fn static_name() -> String {
                format!("{}::{}", module_path!(), stringify!(#name))
            }
        }

        impl StateName for #name {
            fn name(&self) -> String {
                <Self as StaticState>::static_name()
            }
        }
    };

    TokenStream::from(expanded)
}

#[derive(FromDeriveInput, Debug)]
#[darling(attributes(command), forward_attrs(doc))]
struct CommandInfo {
    ident: Ident,
    data: Data<CommandVariant, CommandField>,
}

#[derive(FromVariant, Debug)]
#[darling(attributes(command), forward_attrs(doc))]
struct CommandVariant {
    ident: Ident,
    fields: Fields<CommandField>,
    #[darling(default, rename = "drop_ident")]
    drop_ident_name: bool,
    #[darling(multiple, rename = "name")]
    names: Vec<String>,

    #[darling(default)]
    parser: Option<Path>,

    attrs: Vec<syn::Attribute>,
}

#[derive(FromField, Debug)]
#[darling(attributes(command), forward_attrs(doc))]
struct CommandField {
    ident: Option<Ident>,
    ty: Type,
    #[darling(default)]
    type_name: Option<String>,
    #[darling(default)]
    name: Option<String>,
    #[darling(default)]
    flag: bool,
}

impl CommandField {
    fn flag_cli_name(&self) -> String {
        let base = if let Some(ref n) = self.name {
            n.clone()
        } else if let Some(ref ident) = self.ident {
            ident.to_string()
        } else {
            "_".to_string()
        };
        format!("--{}", base)
    }
}

#[proc_macro_derive(Command, attributes(command))]
pub fn command_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    let info = CommandInfo::from_derive_input(&ast).unwrap();

    let variants = match info.data {
        Data::Enum(variants) => variants,
        _ => panic!("Command can only be derived on enums."),
    };

    let info_matches = variants
        .iter()
        .map(|variant| {
            let ident = &variant.ident;

            let mut names = variant.names.clone();

            let mut desc = vec![];

            for attr in &variant.attrs {
                if let Ok(name_val) = attr.meta.require_name_value()
                    && let Ok(ident) = name_val.path.require_ident()
                    && ident.to_string().as_str() == "doc"
                {
                    desc.push(name_val.value.to_token_stream());
                }
            }

            let desc = quote!({
                let mut x = vec![];
                #(x.push(format!("{}", #desc).trim().to_string());)*
                x
            });

            if !variant.drop_ident_name {
                names.insert(0, to_snake_case(&ident.to_string()));
            }

            if names.is_empty() {
                panic!("command must have at least 1 valid name.");
            }

            let field_name_types = variant
                .fields
                .iter()
                .map(|field| {
                    let base_name = if let Some(ref field_ident) = field.ident {
                        field
                            .name
                            .clone()
                            .unwrap_or_else(|| field_ident.to_string())
                    } else {
                        field.name.clone().unwrap_or_else(|| "_".to_string())
                    };

                    let name = if field.flag {
                        format!("--{}", base_name)
                    } else {
                        base_name
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
                    valid_names: vec![#(#names.to_string()),*],
                    args: vec![#(#field_name_types),*],
                    desc: #desc,
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

    let match_arms = variants
        .iter()
        .map(|variant| {
            let ident = &variant.ident;
            let mut names = variant.names.clone();

            if !variant.drop_ident_name {
                names.insert(0, to_snake_case(&ident.to_string()));
            }

            if names.is_empty() {
                panic!("command must have at least 1 valid name.");
            }

            if let Some(parser_func) = &variant.parser {
                return quote! {
                    #(#names)|* => {
                        Some(#parser_func(val))
                    }
                };
            }

            // Validate: optional positional fields cannot precede required positional fields.
            // Flags are exempt from this rule.
            {
                let mut saw_optional_positional = false;
                for field in variant.fields.iter() {
                    if field.flag {
                        continue;
                    }
                    let is_optional = get_option_inner_type(&field.ty).is_some();
                    if saw_optional_positional && !is_optional {
                        let field_name = field
                            .ident
                            .as_ref()
                            .map(|i| i.to_string())
                            .unwrap_or_else(|| "_".to_string());
                        panic!(
                            "Required positional field `{}` cannot follow an optional positional \
                             field. Mark it as #[command(flag)] if it should be a flag.",
                            field_name
                        );
                    }
                    if is_optional {
                        saw_optional_positional = true;
                    }
                }
            }

            let has_flags = variant.fields.iter().any(|f| f.flag);

            if has_flags {
                let prescan = quote! {
                    let _state = match ::kerbin_core::CommandState::parse(val) {
                        Some(s) => s,
                        None => return None,
                    };
                };

                let positional_count = variant.fields.iter().filter(|f| !f.flag).count();
                let num_required_positional = variant
                    .fields
                    .iter()
                    .filter(|f| !f.flag && get_option_inner_type(&f.ty).is_none())
                    .count();

                let arg_check = if num_required_positional == positional_count {
                    quote! {
                        if _state.positional.len() != #positional_count { return None; }
                    }
                } else {
                    quote! {
                        if _state.positional.len() < #num_required_positional
                            || _state.positional.len() > #positional_count
                        {
                            return None;
                        }
                    }
                };

                let mut arg_idx = 1usize;
                let mut pos_idx = 0usize;

                let field_parsers_and_names = variant
                    .fields
                    .iter()
                    .map(|field| {
                        let ty = &field.ty;
                        let var = syn::Ident::new(
                            &format!("arg_{}", arg_idx),
                            proc_macro2::Span::call_site(),
                        );
                        arg_idx += 1;

                        let field_name_assignment = match variant.fields.style {
                            Style::Struct => {
                                let field_ident = field.ident.as_ref().unwrap();
                                quote! { #field_ident: #var }
                            }
                            Style::Tuple => quote! { #var },
                            Style::Unit => quote! {},
                        };

                        let parser = if field.flag {
                            let flag_name_str = field.flag_cli_name();

                            if is_bool_type(ty) {
                                // --flag → true, absent → false
                                quote! {
                                    let #var = _state.flags.contains_key(#flag_name_str);
                                }
                            } else if get_option_inner_type(ty)
                                .map(is_bool_type)
                                .unwrap_or(false)
                            {
                                // --flag → Some(true), absent → None
                                quote! {
                                    let #var = if _state.flags.contains_key(#flag_name_str) {
                                        Some(true)
                                    } else {
                                        None
                                    };
                                }
                            } else if let Some(inner_ty) = get_option_inner_type(ty)
                                && get_vec_inner_type(inner_ty)
                                    .map(is_token_type)
                                    .unwrap_or(false)
                            {
                                // --flag [tokens] → Some(items), absent → None
                                quote! {
                                    let #var = match _state.flags.get(#flag_name_str) {
                                        Some(Some(Token::List(_items))) => Some(_items.clone()),
                                        _ => None,
                                    };
                                }
                            } else if get_vec_inner_type(ty)
                                .map(is_token_type)
                                .unwrap_or(false)
                            {
                                // --flag [tokens] → items (required)
                                quote! {
                                    let #var = match _state.flags.get(#flag_name_str) {
                                        Some(Some(Token::List(_items))) => _items.clone(),
                                        _ => return None,
                                    };
                                }
                            } else if let Some(inner_ty) = get_option_inner_type(ty)
                                && let Some(list_inner_ty) = get_vec_inner_type(inner_ty)
                            {
                                // --flag [items] → Some(parsed_items), absent → None
                                quote! {
                                    let #var = match _state.flags.get(#flag_name_str) {
                                        Some(Some(Token::List(_items))) => Some(
                                            _items.iter().filter_map(|_t| {
                                                if let Token::Word(_w) = _t {
                                                    _w.parse::<#list_inner_ty>().ok()
                                                } else {
                                                    None
                                                }
                                            }).collect::<Vec<#list_inner_ty>>()
                                        ),
                                        _ => None,
                                    };
                                }
                            } else if let Some(list_inner_ty) = get_vec_inner_type(ty) {
                                // --flag [items] → parsed_items (required)
                                quote! {
                                    let #var = match _state.flags.get(#flag_name_str) {
                                        Some(Some(Token::List(_items))) => _items.iter().filter_map(|_t| {
                                            if let Token::Word(_w) = _t {
                                                _w.parse::<#list_inner_ty>().ok()
                                            } else {
                                                None
                                            }
                                        }).collect::<Vec<#list_inner_ty>>(),
                                        _ => return None,
                                    };
                                }
                            } else if let Some(inner_ty) = get_option_inner_type(ty) {
                                // --flag value → Some(parsed), absent → None
                                quote! {
                                    let #var = match _state.flags.get(#flag_name_str) {
                                        Some(Some(Token::Word(_v))) => Some(match _v.parse::<#inner_ty>() {
                                            Ok(_t) => _t,
                                            Err(_e) => return Some(Err(_e.to_string())),
                                        }),
                                        _ => None,
                                    };
                                }
                            } else {
                                // --flag value → parsed (required)
                                quote! {
                                    let #var = match _state.flags.get(#flag_name_str) {
                                        Some(Some(Token::Word(_v))) => match _v.parse::<#ty>() {
                                            Ok(_t) => _t,
                                            Err(_e) => return Some(Err(_e.to_string())),
                                        },
                                        _ => return None,
                                    };
                                }
                            }
                        } else {
                            let current_pos_idx = pos_idx;
                            pos_idx += 1;

                            if let Some(inner) = get_option_inner_type(ty)
                                && get_vec_inner_type(inner)
                                    .map(is_token_type)
                                    .unwrap_or(false)
                            {
                                quote! {
                                    let #var = match _state.positional.get(#current_pos_idx) {
                                        Some(Token::List(items)) => Some(items.clone()),
                                        _ => None,
                                    };
                                }
                            } else if get_vec_inner_type(ty)
                                .map(is_token_type)
                                .unwrap_or(false)
                            {
                                quote! {
                                    let #var = match _state.positional.get(#current_pos_idx) {
                                        Some(Token::List(items)) => items.clone(),
                                        _ => return None,
                                    };
                                }
                            } else if let Some(inner_ty) = get_vec_inner_type(ty) {
                                quote! {
                                    let #var = match _state.positional.get(#current_pos_idx) {
                                        Some(Token::List(items)) => {
                                            items.iter().filter_map(|t| {
                                                if let Token::Word(s) = t {
                                                    s.parse::<#inner_ty>().ok()
                                                } else {
                                                    None
                                                }
                                            }).collect::<Vec<#inner_ty>>()
                                        }
                                        _ => return None,
                                    };
                                }
                            } else if is_token_type(ty) {
                                quote! {
                                    let #var = match _state.positional.get(#current_pos_idx) {
                                        Some(t) => t.clone(),
                                        None => return None,
                                    };
                                }
                            } else if let Some(inner_ty) = get_option_inner_type(ty) {
                                quote! {
                                    let #var = if let Some(Token::Word(s)) = _state.positional.get(#current_pos_idx) {
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
                                    let #var = match _state.positional.get(#current_pos_idx) {
                                        Some(Token::Word(s)) => match s.parse::<#ty>() {
                                            Ok(t) => t,
                                            Err(e) => return Some(Err(e.to_string())),
                                        },
                                        _ => return None,
                                    };
                                }
                            }
                        };

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
                    #(#names)|* => {
                        #prescan
                        #arg_check
                        #(#field_parsers)*
                        Some(Ok(Box::new(Self::#ident #fields)))
                    }
                }
            } else {
                // Original positional-only code path.
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

                        let parser = if let Some(inner) = get_option_inner_type(ty)
                            && get_vec_inner_type(inner).map(is_token_type).unwrap_or(false)
                        {
                            // Option<Vec<Token>>: match List or absent, never parse
                            quote! {
                                let #var = match val.get(#idx_usize) {
                                    Some(Token::List(items)) => Some(items.clone()),
                                    _ => None,
                                };
                            }
                        } else if get_vec_inner_type(ty).map(is_token_type).unwrap_or(false) {
                            // Vec<Token>: clone items directly from Token::List
                            quote! {
                                let #var = match val.get(#idx_usize) {
                                    Some(Token::List(items)) => items.clone(),
                                    _ => return None,
                                };
                            }
                        } else if let Some(inner_ty) = get_vec_inner_type(ty) {
                            // Vec<T>: expect a Token::List at this position
                            quote! {
                                let #var = match val.get(#idx_usize) {
                                    Some(Token::List(items)) => {
                                        items.iter().filter_map(|t| {
                                            if let Token::Word(s) = t {
                                                s.parse::<#inner_ty>().ok()
                                            } else {
                                                None
                                            }
                                        }).collect::<Vec<#inner_ty>>()
                                    }
                                    _ => return None,
                                };
                            }
                        } else if is_token_type(ty) {
                            // Token: accept any token at this position
                            quote! {
                                let #var = match val.get(#idx_usize) {
                                    Some(t) => t.clone(),
                                    None => return None,
                                };
                            }
                        } else if let Some(inner_ty) = get_option_inner_type(ty) {
                            // Option<T>: optional Token::Word at this position
                            quote! {
                                let #var = if let Some(Token::Word(s)) = val.get(#idx_usize) {
                                    Some(match s.parse::<#inner_ty>() {
                                        Ok(t) => t,
                                        Err(e) => return Some(Err(e.to_string())),
                                    })
                                } else {
                                    None
                                };
                            }
                        } else {
                            // Regular T: expect a Token::Word at this position
                            quote! {
                                let #var = match val.get(#idx_usize) {
                                    Some(Token::Word(s)) => match s.parse::<#ty>() {
                                        Ok(t) => t,
                                        Err(e) => return Some(Err(e.to_string())),
                                    },
                                    _ => return None,
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
                    #(#names)|* => {
                        #arg_check
                        #(#field_parsers)*
                        Some(Ok(Box::new(Self::#ident #fields)))
                    }
                }
            }
        })
        .collect::<Vec<_>>();

    let command_from_str_impl = {
        let ident = &info.ident;
        quote! {
            impl<__S: Send + Sync + 'static> CommandFromStr<__S> for #ident
            where
                #ident: Command<__S>,
            {
                fn from_str(val: &[Token]) -> Option<Result<Box<dyn Command<__S>>, String>> {
                    match val.get(0) {
                        Some(Token::Word(s)) => match s.as_str() {
                            #(#match_arms),*
                            _ => None,
                        },
                        _ => None,
                    }
                }
            }
        }
    };

    let command_any_impl = {
        let ident = &info.ident;
        quote! {
            impl CommandAny for #ident {
                fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
                    self
                }
            }
        }
    };

    let expanded = quote! {
        #as_command_info_impl
        #command_from_str_impl
        #command_any_impl
    };

    expanded.into()
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars = s.chars();
    for c in chars {
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
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Option"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty);
    }
    None
}

fn is_bool_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty
        && let Some(seg) = tp.path.segments.last()
    {
        return seg.ident == "bool";
    }
    false
}

fn is_token_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty
        && let Some(seg) = tp.path.segments.last()
    {
        return seg.ident == "Token";
    }
    false
}

fn get_vec_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Vec"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty);
    }
    None
}
