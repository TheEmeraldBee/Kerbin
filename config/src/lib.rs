#![allow(improper_ctypes_definitions)]

use kerbin_core::{ascii_forge::prelude::*, *};

pub async fn hello(chunk: Chunk<BufferChunk>) {
    let mut chunk = chunk.get().unwrap();

    render!(chunk, (0, 0) => [ "Hello, World"] );
}

#[unsafe(no_mangle)]
pub fn init(state: &mut State) {
    init_conf();

    kerbin_tree_sitter::init(state);
    kerbin_tree_sitter::register_lang(state, "rust", ["rs"]);
    kerbin_tree_sitter::register_lang(state, "toml", ["toml"]);

    // Test a hello world file for fun
    state.on_hook(RenderFiletype::new("hi")).system(hello);
}
