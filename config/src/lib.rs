#![allow(improper_ctypes_definitions)]

use std::iter::empty;

use kerbin_core::*;

pub async fn hi_renderer(bufs: Res<Buffers>) {
    get!(bufs);

    let buf = bufs.cur_buffer();
    let mut buf = buf.write().unwrap();

    buf.clear_extmark_ns("custom::hi");

    buf.add_extmark(
        "custom::hi",
        0,
        0,
        vec![ExtmarkDecoration::VirtText {
            text: "Hello, World".to_string(),
            hl: None,
        }],
    );
}

#[unsafe(no_mangle)]
pub fn init(state: &mut State) {
    init_conf();

    kerbin_tree_sitter::init(state);
    kerbin_tree_sitter::register_lang(state, "rust", ["rs"]);
    kerbin_tree_sitter::register_lang(state, "toml", ["toml"]);

    kerbin_tree_sitter::register_lang(state, "markdown", ["md"]);
    kerbin_tree_sitter::register_lang(state, "markdown-inline", empty::<String>());

    state.on_hook(RenderFiletype::new("hi")).system(hi_renderer);
}
