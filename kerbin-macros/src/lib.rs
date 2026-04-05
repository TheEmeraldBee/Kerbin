use darling::{
    ast::{Data, Fields, Style},
    *,
};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    DeriveInput, Expr, Ident, LitStr, Path, Token, Type, bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

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
    #[darling(default)]
    ignore: bool,
}

impl CommandVariant {
    fn resolved_names(&self) -> Vec<String> {
        let mut names = self.names.clone();
        if !self.drop_ident_name {
            names.insert(0, to_snake_case(&self.ident.to_string()));
        }
        assert!(
            !names.is_empty(),
            "command must have at least 1 valid name."
        );
        names
    }

    fn doc_lines(&self) -> TokenStream2 {
        let lines = self.attrs.iter().filter_map(|attr| {
            let nv = attr.meta.require_name_value().ok()?;
            let id = nv.path.require_ident().ok()?;
            (id.to_string() == "doc").then(|| nv.value.to_token_stream())
        });
        quote!({
            let mut x = vec![];
            #(x.push(format!("{}", #lines).trim().to_string());)*
            x
        })
    }

    fn validate_fields(&self) {
        let mut saw_optional = false;
        for field in self.fields.iter() {
            if field.flag {
                continue;
            }
            let is_opt = get_option_inner_type(&field.ty).is_some();
            if saw_optional && !is_opt {
                panic!(
                    "Required positional field `{}` cannot follow an optional positional field. \
                     Mark it as #[command(flag)] if it should be a flag.",
                    field.field_base_name()
                );
            }
            saw_optional |= is_opt;
        }
    }
}

impl CommandField {
    fn field_base_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            self.ident
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_else(|| "_".to_string())
        })
    }

    fn flag_cli_name(&self) -> String {
        format!("--{}", self.field_base_name())
    }

    fn field_assignment(&self, style: Style, var: &Ident) -> TokenStream2 {
        match style {
            Style::Struct => {
                let id = self
                    .ident
                    .as_ref()
                    .expect("struct fields always have identifiers");
                quote! { #id: #var }
            }
            Style::Tuple => quote! { #var },
            Style::Unit => quote! {},
        }
    }
}

enum FieldSource<'a> {
    /// Named flag
    Flag(&'a str),
    /// Positional slot at a fixed index
    Positional(usize),
}

fn emit_field_parser(var: &Ident, ty: &Type, source: FieldSource<'_>, ignore: bool) -> TokenStream2 {
    if ignore {
        return emit_field_parser_ignore(var, ty, source);
    }
    match source {
        FieldSource::Flag(flag_name) => {
            if is_bool_type(ty) {
                quote! { let #var = _state.flags.contains_key(#flag_name); }
            } else if get_option_inner_type(ty).map(is_bool_type).unwrap_or(false) {
                quote! {
                    let #var = if _state.flags.contains_key(#flag_name) { Some(true) } else { None };
                }
            } else if let Some(inner) = get_option_inner_type(ty)
                && get_vec_inner_type(inner)
                    .map(is_token_type)
                    .unwrap_or(false)
            {
                quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::List(_items))) => Some(_items.clone()),
                        _ => None,
                    };
                }
            } else if get_vec_inner_type(ty).map(is_token_type).unwrap_or(false) {
                quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::List(_items))) => _items.clone(),
                        _ => return None,
                    };
                }
            } else if let Some(inner) = get_option_inner_type(ty)
                && let Some(list_inner) = get_vec_inner_type(inner)
            {
                quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::List(_items))) => Some(
                            _items.iter().filter_map(|_t| {
                                if let Token::Word(_w) = _t { _w.parse::<#list_inner>().ok() } else { None }
                            }).collect::<Vec<#list_inner>>()
                        ),
                        _ => None,
                    };
                }
            } else if let Some(list_inner) = get_vec_inner_type(ty) {
                quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::List(_items))) => _items.iter().filter_map(|_t| {
                            if let Token::Word(_w) = _t { _w.parse::<#list_inner>().ok() } else { None }
                        }).collect::<Vec<#list_inner>>(),
                        _ => return None,
                    };
                }
            } else if let Some(inner) = get_option_inner_type(ty) {
                quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::Word(_v))) => Some(match _v.parse::<#inner>() {
                            Ok(_t) => _t,
                            Err(_e) => return Some(Err(_e.to_string())),
                        }),
                        _ => None,
                    };
                }
            } else {
                quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::Word(_v))) => match _v.parse::<#ty>() {
                            Ok(_t) => _t,
                            Err(_e) => return Some(Err(_e.to_string())),
                        },
                        _ => return None,
                    };
                }
            }
        }

        FieldSource::Positional(i) => {
            if let Some(inner) = get_option_inner_type(ty)
                && get_vec_inner_type(inner)
                    .map(is_token_type)
                    .unwrap_or(false)
            {
                quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(Token::List(items)) => Some(items.clone()),
                        _ => None,
                    };
                }
            } else if get_vec_inner_type(ty).map(is_token_type).unwrap_or(false) {
                quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(Token::List(items)) => items.clone(),
                        _ => return None,
                    };
                }
            } else if let Some(inner) = get_vec_inner_type(ty) {
                quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(Token::List(items)) => items.iter().filter_map(|t| {
                            if let Token::Word(s) = t { s.parse::<#inner>().ok() } else { None }
                        }).collect::<Vec<#inner>>(),
                        _ => return None,
                    };
                }
            } else if is_token_type(ty) {
                quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(t) => t.clone(),
                        None => return None,
                    };
                }
            } else if let Some(inner) = get_option_inner_type(ty) {
                quote! {
                    let #var = if let Some(Token::Word(s)) = _state.positional.get(#i) {
                        Some(match s.parse::<#inner>() {
                            Ok(t) => t,
                            Err(e) => return Some(Err(e.to_string())),
                        })
                    } else {
                        None
                    };
                }
            } else {
                quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(Token::Word(s)) => match s.parse::<#ty>() {
                            Ok(t) => t,
                            Err(e) => return Some(Err(e.to_string())),
                        },
                        _ => return None,
                    };
                }
            }
        }
    }
}

/// Generates field-parsing code for fields marked `#[command(ignore)]`.
///
/// The token at this slot is kept raw (unexpanded). For `Token`/`Option<Token>`/`Vec<Token>` types
/// the token is used directly. For string-like types it is serialized back with `token_to_string`
/// so the caller receives the original source form (e.g. `%var`, `$(cmd)`).
fn emit_field_parser_ignore(var: &Ident, ty: &Type, source: FieldSource<'_>) -> TokenStream2 {
    match source {
        FieldSource::Flag(flag_name) => {
            if is_bool_type(ty) || get_option_inner_type(ty).map(is_bool_type).unwrap_or(false) {
                // bool/Option<bool> flag: presence-only — ignore has no effect on these.
                if is_bool_type(ty) {
                    return quote! { let #var = _state.flags.contains_key(#flag_name); };
                } else {
                    return quote! {
                        let #var = if _state.flags.contains_key(#flag_name) { Some(true) } else { None };
                    };
                }
            }

            if is_token_type(ty) {
                return quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(t)) => t.clone(),
                        _ => return None,
                    };
                };
            }

            if let Some(inner) = get_option_inner_type(ty) {
                if is_token_type(inner) {
                    return quote! {
                        let #var = match _state.flags.get(#flag_name) {
                            Some(Some(t)) => Some(t.clone()),
                            _ => None,
                        };
                    };
                }
                if get_vec_inner_type(inner).map(is_token_type).unwrap_or(false) {
                    return quote! {
                        let #var = match _state.flags.get(#flag_name) {
                            Some(Some(Token::List(_items))) => Some(_items.clone()),
                            _ => None,
                        };
                    };
                }
                if let Some(list_inner) = get_vec_inner_type(inner) {
                    return quote! {
                        let #var = match _state.flags.get(#flag_name) {
                            Some(Some(Token::List(_items))) => Some(
                                _items.iter().filter_map(|_t| {
                                    ::kerbin_core::token_to_string(_t).parse::<#list_inner>().ok()
                                }).collect::<Vec<#list_inner>>()
                            ),
                            _ => None,
                        };
                    };
                }
                return quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(_t)) => Some(match ::kerbin_core::token_to_string(_t).parse::<#inner>() {
                            Ok(_v) => _v,
                            Err(_e) => return Some(Err(_e.to_string())),
                        }),
                        _ => None,
                    };
                };
            }

            if get_vec_inner_type(ty).map(is_token_type).unwrap_or(false) {
                return quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::List(_items))) => _items.clone(),
                        _ => return None,
                    };
                };
            }

            if let Some(list_inner) = get_vec_inner_type(ty) {
                return quote! {
                    let #var = match _state.flags.get(#flag_name) {
                        Some(Some(Token::List(_items))) => _items.iter().filter_map(|_t| {
                            ::kerbin_core::token_to_string(_t).parse::<#list_inner>().ok()
                        }).collect::<Vec<#list_inner>>(),
                        _ => return None,
                    };
                };
            }

            quote! {
                let #var = match _state.flags.get(#flag_name) {
                    Some(Some(_t)) => match ::kerbin_core::token_to_string(_t).parse::<#ty>() {
                        Ok(_v) => _v,
                        Err(_e) => return Some(Err(_e.to_string())),
                    },
                    _ => return None,
                };
            }
        }

        FieldSource::Positional(i) => {
            if is_token_type(ty) {
                return quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(t) => t.clone(),
                        None => return None,
                    };
                };
            }

            if let Some(inner) = get_option_inner_type(ty) {
                if is_token_type(inner) {
                    return quote! {
                        let #var = _state.positional.get(#i).cloned();
                    };
                }
                if get_vec_inner_type(inner).map(is_token_type).unwrap_or(false) {
                    return quote! {
                        let #var = match _state.positional.get(#i) {
                            Some(Token::List(items)) => Some(items.clone()),
                            _ => None,
                        };
                    };
                }
                if let Some(list_inner) = get_vec_inner_type(inner) {
                    return quote! {
                        let #var = match _state.positional.get(#i) {
                            Some(Token::List(items)) => Some(
                                items.iter().filter_map(|t| {
                                    ::kerbin_core::token_to_string(t).parse::<#list_inner>().ok()
                                }).collect::<Vec<#list_inner>>()
                            ),
                            _ => None,
                        };
                    };
                }
                return quote! {
                    let #var = if let Some(t) = _state.positional.get(#i) {
                        Some(match ::kerbin_core::token_to_string(t).parse::<#inner>() {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e.to_string())),
                        })
                    } else {
                        None
                    };
                };
            }

            if get_vec_inner_type(ty).map(is_token_type).unwrap_or(false) {
                return quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(Token::List(items)) => items.clone(),
                        _ => return None,
                    };
                };
            }

            if let Some(list_inner) = get_vec_inner_type(ty) {
                return quote! {
                    let #var = match _state.positional.get(#i) {
                        Some(Token::List(items)) => items.iter().filter_map(|t| {
                            ::kerbin_core::token_to_string(t).parse::<#list_inner>().ok()
                        }).collect::<Vec<#list_inner>>(),
                        _ => return None,
                    };
                };
            }

            quote! {
                let #var = match _state.positional.get(#i) {
                    Some(t) => match ::kerbin_core::token_to_string(t).parse::<#ty>() {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e.to_string())),
                    },
                    None => return None,
                };
            }
        }
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

    let info_matches: Vec<_> = variants
        .iter()
        .map(|v| {
            let names = v.resolved_names();
            let desc = v.doc_lines();
            let field_name_types: Vec<_> = v
                .fields
                .iter()
                .map(|f| {
                    let name = if f.flag {
                        f.flag_cli_name()
                    } else {
                        f.field_base_name()
                    };
                    let field_ty = &f.ty;
                    let type_name = f
                        .type_name
                        .clone()
                        .unwrap_or_else(|| quote!(#field_ty).to_string());
                    quote! { (#name.to_string(), #type_name.to_string()) }
                })
                .collect();

            let mut pos_idx = 0usize;
            let ignore_positional: Vec<usize> = v
                .fields
                .iter()
                .filter_map(|f| {
                    if f.flag {
                        return None;
                    }
                    let idx = pos_idx;
                    pos_idx += 1;
                    if f.ignore { Some(idx) } else { None }
                })
                .collect();
            let ignore_flag_names: Vec<String> = v
                .fields
                .iter()
                .filter(|f| f.flag && f.ignore)
                .map(|f| f.flag_cli_name())
                .collect();

            quote! {
                CommandInfo {
                    valid_names: vec![#(#names.to_string()),*],
                    args: vec![#(#field_name_types),*],
                    desc: #desc,
                    ignore_positional: vec![#(#ignore_positional),*],
                    ignore_flags: vec![#(#ignore_flag_names.to_string()),*],
                }
            }
        })
        .collect();

    let match_arms: Vec<_> = variants
        .iter()
        .map(|variant| {
            let ident = &variant.ident;
            let names = variant.resolved_names();

            // Delegate entirely to a custom parser if provided.
            if let Some(parser_func) = &variant.parser {
                return quote! {
                    #(#names)|* => Some(#parser_func(val))
                };
            }

            variant.validate_fields();

            let prescan = quote! {
                let _state = match ::kerbin_core::CommandState::parse(val) {
                    Some(s) => s,
                    None => return None,
                };
            };

            let num_pos = variant.fields.iter().filter(|f| !f.flag).count();
            let num_req = variant
                .fields
                .iter()
                .filter(|f| !f.flag && get_option_inner_type(&f.ty).is_none())
                .count();

            let arg_check = if num_req == num_pos {
                quote! { if _state.positional.len() != #num_pos { return None; } }
            } else {
                quote! {
                    if _state.positional.len() < #num_req || _state.positional.len() > #num_pos {
                        return None;
                    }
                }
            };

            let mut pos_idx = 0usize;
            let mut arg_n = 1usize;

            let (parsers, assignments): (Vec<_>, Vec<_>) = variant
                .fields
                .iter()
                .map(|field| {
                    let ty = &field.ty;
                    let var = Ident::new(&format!("arg_{arg_n}"), proc_macro2::Span::call_site());
                    arg_n += 1;

                    let source = if field.flag {
                        let name = field.flag_cli_name();
                        FieldSource::Flag(Box::leak(name.into_boxed_str()))
                    } else {
                        let idx = pos_idx;
                        pos_idx += 1;
                        FieldSource::Positional(idx)
                    };

                    let parser = emit_field_parser(&var, ty, source, field.ignore);
                    let assignment = field.field_assignment(variant.fields.style, &var);
                    (parser, assignment)
                })
                .unzip();

            let fields = match variant.fields.style {
                Style::Struct => quote! { { #(#assignments),* } },
                Style::Tuple => quote! { ( #(#assignments),* ) },
                Style::Unit => quote! {},
            };

            quote! {
                #(#names)|* => {
                    #prescan
                    #arg_check
                    #(#parsers)*
                    Some(Ok(Box::new(Self::#ident #fields)))
                }
            }
        })
        .collect();

    let enum_ident = &info.ident;
    let expanded = quote! {
        impl AsCommandInfo for #enum_ident {
            fn infos() -> Vec<CommandInfo> {
                vec![#(#info_matches),*]
            }
        }

        impl<__S: Send + Sync + 'static> CommandFromStr<__S> for #enum_ident
        where
            #enum_ident: Command<__S>,
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

        impl CommandAny for #enum_ident {
            fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
                self
            }
        }
    };

    expanded.into()
}

struct HookEntry {
    hook: Expr,
    system: Path,
}

struct EventEntry {
    event: Path,
    system: Path,
}

#[derive(Default)]
struct PluginDef {
    name: Option<LitStr>,
    init_as: Option<Ident>,
    state: Vec<Path>,
    commands: Vec<Path>,
    hooks: Vec<HookEntry>,
    events: Vec<EventEntry>,
}

/// Parse a bracketed list of `A => B` pairs.
fn parse_arrow_pairs<A, B>(content: ParseStream) -> syn::Result<Vec<(A, B)>>
where
    A: Parse,
    B: Parse,
{
    let inner;
    bracketed!(inner in content);
    let mut pairs = vec![];
    while !inner.is_empty() {
        let a: A = inner.parse()?;
        inner.parse::<Token![=>]>()?;
        let b: B = inner.parse()?;
        pairs.push((a, b));
        if inner.peek(Token![,]) {
            inner.parse::<Token![,]>()?;
        }
    }
    Ok(pairs)
}

/// Parse a bracketed comma-separated list of `T`.
fn parse_bracketed_list<T: Parse>(content: ParseStream) -> syn::Result<Vec<T>> {
    let inner;
    bracketed!(inner in content);
    let items: Punctuated<T, Token![,]> = inner.parse_terminated(T::parse, Token![,])?;
    Ok(items.into_iter().collect())
}

impl Parse for PluginDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut def = PluginDef::default();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![:]>()?;

            match key.to_string().as_str() {
                "name" => def.name = Some(input.parse()?),
                "init_as" => def.init_as = Some(input.parse()?),
                "state" => def.state = parse_bracketed_list(input)?,
                "commands" => def.commands = parse_bracketed_list(input)?,
                "hooks" => {
                    def.hooks = parse_arrow_pairs::<Expr, Path>(input)?
                        .into_iter()
                        .map(|(hook, system)| HookEntry { hook, system })
                        .collect();
                }
                "events" => {
                    def.events = parse_arrow_pairs::<Path, Path>(input)?
                        .into_iter()
                        .map(|(event, system)| EventEntry { event, system })
                        .collect();
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown field `{other}` — expected one of: \
                             name, init_as, state, commands, hooks, events"
                        ),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(def)
    }
}

/// Generates `init` and `register_commands` for a Kerbin plugin.
///
/// ```ignore
/// define_plugin! {
///     name: "my-plugin",
///
///     state: [
///         MyState,
///     ],
///
///     commands: [
///         MyCommand,
///     ],
///
///     hooks: [
///         hooks::Update => my_update_system,
///     ],
///
///     events: [
///         SaveEvent => on_file_saved,
///     ],
/// }
/// ```
///
/// Generates:
/// - `pub async fn register_commands(state: &mut State)` — registers all listed commands.
/// - `pub async fn init(state: &mut State)` — registers state (via `Default`), calls
///   `register_commands`, attaches hook systems, and subscribes to events.
#[proc_macro]
pub fn define_plugin(input: TokenStream) -> TokenStream {
    let def = parse_macro_input!(input as PluginDef);

    let state_inits: Vec<TokenStream2> = def
        .state
        .iter()
        .map(|ty| quote! { state.state(<#ty as Default>::default()); })
        .collect();

    let command_registrations: Vec<TokenStream2> = def
        .commands
        .iter()
        .map(|ty| quote! { registry.register::<#ty>(); })
        .collect();

    let hook_registrations: Vec<TokenStream2> = def
        .hooks
        .iter()
        .map(|e| {
            let hook = &e.hook;
            let system = &e.system;
            quote! { state.on_hook(#hook).system(#system); }
        })
        .collect();

    let event_subscriptions: Vec<TokenStream2> = def
        .events
        .iter()
        .map(|e| {
            let event = &e.event;
            let system = &e.system;
            quote! {
                ::kerbin_core::EVENT_BUS
                    .subscribe::<#event>()
                    .await
                    .system(#system);
            }
        })
        .collect();

    let register_commands = quote! {
        pub fn register_commands(registry: &mut ::kerbin_core::CommandRegistry) {
            #(#command_registrations)*
        }
    };

    let init_register_commands = if command_registrations.is_empty() {
        quote! {}
    } else {
        quote! {
            {
                let mut registry = state
                    .lock_state::<::kerbin_core::CommandRegistry>()
                    .await;
                #(#command_registrations)*
            }
        }
    };

    let plugin_registration = def.name.as_ref().map(|name| {
        quote! {
            state
                .lock_state::<::kerbin_core::PluginRegistry>()
                .await
                .register(#name);
        }
    });

    let init_fn_name = def
        .init_as
        .clone()
        .unwrap_or_else(|| Ident::new("init", proc_macro2::Span::call_site()));

    let expanded = quote! {
        #register_commands

        pub async fn #init_fn_name(state: &mut ::kerbin_core::State) {
            #plugin_registration
            #(#state_inits)*
            #init_register_commands
            #(#hook_registrations)*
            #(#event_subscriptions)*
        }
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
