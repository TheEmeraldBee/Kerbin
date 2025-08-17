use darling::*;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Item, ItemFn, Visibility, parse_macro_input};

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
