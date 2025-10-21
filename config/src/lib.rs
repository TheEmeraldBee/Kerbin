#![allow(improper_ctypes_definitions)]

// Sample config file for basic plugin systems

use std::iter::empty;

use kerbin_core::*;
use kerbin_lsp::LanguageInfo;

pub async fn init(state: &mut State) {
    // Initialize the tree-sitter plugin
    kerbin_tree_sitter::init(state).await;
    // Add a couple languages
    // Defined by language name (tree-sitter-{language})
    // Exts are file-paths that will use that grammar
    kerbin_tree_sitter::register_lang(state, "rust", ["rs"]).await;
    kerbin_tree_sitter::register_lang(state, "toml", ["toml"]).await;

    kerbin_tree_sitter::register_lang(state, "markdown", ["md"]).await;
    kerbin_tree_sitter::register_lang(state, "markdown-inline", empty::<String>()).await;

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
