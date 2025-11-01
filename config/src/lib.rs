#![allow(improper_ctypes_definitions)]

// Sample config file for basic plugin systems

use kerbin_core::*;

pub async fn init(state: &mut State) {
    // Initialize the tree-sitter plugin
    kerbin_tree_sitter::init(state).await;

    // Initialize the lsp plugin
    kerbin_lsp::init(state).await;
}
