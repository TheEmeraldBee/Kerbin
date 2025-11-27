#![allow(improper_ctypes_definitions)]

// Sample config file for basic plugin systems

use kerbin_core::*;
use kerbin_lsp::LangInfo;

pub async fn init(state: &mut State) {
    // Initialize the tree-sitter plugin
    kerbin_tree_sitter::init(state).await;

    // Initialize the lsp plugin
    kerbin_lsp::init(state).await;

    kerbin_lsp::register_lang(
        state,
        "rust",
        ["rs"],
        LangInfo::new("rust-analyzer")
            .with_root("Cargo.toml") // Root paths
            .with_root("Cargo.lock"),
    )
    .await;
}
