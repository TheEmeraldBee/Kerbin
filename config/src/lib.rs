#![allow(improper_ctypes_definitions)]

use std::iter::empty;

use kerbin_core::*;

#[unsafe(no_mangle)]
pub fn init(state: &mut State) {
    init_conf();

    kerbin_tree_sitter::init(state);
    kerbin_tree_sitter::register_lang(state, "rust", ["rs"]);
    kerbin_tree_sitter::register_lang(state, "toml", ["toml"]);
    kerbin_tree_sitter::register_lang(state, "markdown", ["md"]);
    kerbin_tree_sitter::register_lang(state, "markdown-inline", ["markdown-inline"]);
}
