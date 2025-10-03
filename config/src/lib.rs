#![allow(improper_ctypes_definitions)]

use std::{fmt::Display, iter::empty, sync::Arc};

use kerbin_core::{
    ascii_forge::window::{
        Print,
        crossterm::{cursor::MoveTo, execute},
    },
    *,
};

pub fn large_text_render_method(
    text: impl Display + Send + Sync + 'static,
    size: u16,
) -> RenderFunc {
    Arc::new(Box::new(move |w, p| {
        let io = w.io();

        execute!(
            io,
            MoveTo(p.x, p.y),
            Print(format!("\x1b]66;s={};{}\x07", size, text))
        )
        .unwrap()
    }))
}

pub async fn hi_renderer(bufs: Res<Buffers>) {
    get!(bufs);

    let buf = bufs.cur_buffer();
    let rndr = &mut buf.write().unwrap().renderer;

    rndr.clear_extmark_ns("custom::hi");

    rndr.add_extmark(
        "custom::hi",
        0,
        0,
        vec![ExtmarkDecoration::VirtText {
            text: "Hello, World".to_string(),
            hl: None,
        }],
    );

    // Test Rendering a Kitty Large Text Element
    rndr.add_extmark(
        "custom::hi",
        0,
        0,
        vec![ExtmarkDecoration::FullElement {
            height: 2,
            func: large_text_render_method("Hello, World", 2),
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

    state
        .on_hook(hooks::UpdateFiletype::new("hi"))
        .system(hi_renderer);
}
