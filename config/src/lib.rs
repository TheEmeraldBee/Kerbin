#![allow(improper_ctypes_definitions)]

// Sample config file for basic plugin systems

use kerbin_core::*;
use kerbin_lsp::LanguageInfo;

pub async fn init(state: &mut State) {
    // Initialize the tree-sitter plugin
    kerbin_tree_sitter::init(state).await;

    // Install all default languages from the plugin
    kerbin_tree_sitter::register_default(state).await;

    // Initialize the lsp plugin
    kerbin_lsp::init(state).await;

    // Register a language with defined roots.
    // Name is just a common name to reference by.
    // Can be anything, prefer using the tree-sitter name when possible
    kerbin_lsp::register_lang(
        state,
        "rust",
        ["rs"],
        LanguageInfo::new("rust-analyzer") // Command to run
            .with_root("Cargo.toml") // Root paths
            .with_root("Cargo.lock"),
    )
    .await;
}
