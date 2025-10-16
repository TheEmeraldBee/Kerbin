#![allow(improper_ctypes_definitions)]

use std::{iter::empty, sync::Arc};

use kerbin_core::{ascii_forge::prelude::*, *};
use kerbin_lsp::LanguageInfo;

pub async fn hi_renderer(bufs: ResMut<Buffers>) {
    get!(mut bufs);

    let mut buf = bufs.cur_buffer_mut().await;
    let rndr = &mut buf.renderer;

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

    let mut elem_buffer = Buffer::new((50, 5));
    render!(elem_buffer, (0, 0) => ["Buffer Text".on(Color::Red)]);

    rndr.add_extmark(
        "custom::hi",
        0,
        0,
        vec![ExtmarkDecoration::FullElement {
            elem: Arc::new(elem_buffer),
        }],
    );
}

pub async fn init(state: &mut State) {
    kerbin_tree_sitter::init(state).await;
    kerbin_tree_sitter::register_lang(state, "rust", ["rs"]).await;
    kerbin_tree_sitter::register_lang(state, "toml", ["toml"]).await;

    kerbin_tree_sitter::register_lang(state, "markdown", ["md"]).await;
    kerbin_tree_sitter::register_lang(state, "markdown-inline", empty::<String>()).await;

    kerbin_lsp::init(state).await;
    kerbin_lsp::register_lang(
        state,
        "rust",
        ["rs"],
        LanguageInfo::new("rust-analyzer")
            .with_root("Cargo.toml")
            .with_root("Cargo.lock"),
    )
    .await;

    state
        .on_hook(hooks::UpdateFiletype::new("hi"))
        .system(hi_renderer);
}
