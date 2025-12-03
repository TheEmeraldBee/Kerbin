use std::sync::Arc;

use kerbin_core::{
    ascii_forge::{prelude::*, widgets::Border},
    *,
};
use lsp_types::{
    Hover, HoverContents, HoverParams, LanguageString, MarkedString, Position,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};

use crate::{JsonRpcMessage, LspManager, OpenedFile};
use kerbin_tree_sitter::{grammar_manager::GrammarManager, state::highlight_text};

pub struct HoverInfo {
    pub pending_request: i32,

    pub hover: Option<Hover>,

    pub position: usize,
}

/// The hover info stored in each text buffer
#[derive(State, Default)]
pub struct HoverState {
    pub info: Option<HoverInfo>,
}

#[derive(Command)]
pub enum HoverCommand {
    #[command]
    /// Request the display of a hover at the cursor's position
    Hover,
}

#[async_trait::async_trait]
impl Command for HoverCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Hover => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut lsps = state.lock_state::<LspManager>().await;

                let mut buf = bufs.cur_buffer_mut().await;

                let Some(file) = buf.get_state::<OpenedFile>().await else {
                    return false;
                };

                let client = lsps
                    .get_or_create_client(&file.lang)
                    .await
                    .expect("Lsp should exist");

                let cursor = buf.primary_cursor();
                let cursor_byte = cursor.get_cursor_byte().min(buf.len_bytes());

                let line = buf.byte_to_line_clamped(cursor_byte);
                let character = cursor_byte - buf.line_to_byte_clamped(line);

                // Create hover request parameters
                let params = HoverParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier {
                            uri: file.uri.clone(),
                        },
                        position: Position::new(line as u32, character as u32),
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                };

                // Send the hover request
                let id = client.request("textDocument/hover", params).await.unwrap();

                let mut state = buf.get_or_insert_state_mut(HoverState::default).await;

                state.info = Some(HoverInfo {
                    pending_request: id,
                    hover: None,
                    position: cursor_byte,
                });
            }
        }

        true
    }
}

pub async fn render_hover(
    buffers: ResMut<Buffers>,
    grammars: ResMut<GrammarManager>,
    config: Res<ConfigFolder>,
    theme: Res<Theme>,
    log: Res<LogSender>,
) {
    get!(mut buffers, mut grammars, config, theme, log);

    let mut buf = buffers.cur_buffer_mut().await;

    let Some(mut state) = buf.get_state_mut::<HoverState>().await else {
        return;
    };

    buf.renderer.clear_extmark_ns("lsp::hover");

    let Some(info) = state.info.as_ref() else {
        return;
    };

    if buf.primary_cursor().get_cursor_byte() != info.position {
        state.info = None;
        return;
    }

    let Some(hover) = info.hover.as_ref() else {
        return;
    };

    let text = match &hover.contents {
        HoverContents::Scalar(t) => extract_hover_markup(t),
        HoverContents::Array(a) => a
            .iter()
            .map(extract_hover_markup)
            .collect::<Vec<String>>()
            .join("\n\n"),
        HoverContents::Markup(m) => m.value.clone(),
    };

    let highlighted = highlight_text(
        &text,
        "markdown",
        &mut grammars,
        &config.0,
        &theme,
        &log,
    );

    // Calculate dimensions
    let mut width = 0;
    let mut height = 1;
    let mut current_line_width = 0;

    for (part, _) in &highlighted {
        for char in part.chars() {
            if char == '\n' {
                width = width.max(current_line_width);
                current_line_width = 0;
                height += 1;
            } else {
                current_line_width += 1;
            }
        }
    }
    width = width.max(current_line_width);

    let mut text_buf = Buffer::new(vec2(width as u16, height as u16));
    let mut x = 0;
    let mut y = 0;

    for (part, style) in highlighted {
        for char in part.chars() {
            if char == '\n' {
                x = 0;
                y += 1;
                continue;
            }
            text_buf.set(vec2(x, y), Cell::new(char.to_string(), style));
            x += 1;
        }
    }

    let mut render = Buffer::new(text_buf.size() + vec2(2, 2));

    render!(render,
        (0, 0) => [ Border::rounded(text_buf.size().x + 2, text_buf.size().y + 2) ],
        (1, 1) => [ text_buf ]
    );

    buf.add_extmark(
        ExtmarkBuilder::new("lsp::hover", info.position)
            .with_priority(5)
            .with_decoration(ExtmarkDecoration::OverlayElement {
                offset: vec2(1, 1),
                elem: Arc::new(render),
                z_index: 5,
                clip_to_viewport: true,
                positioning: OverlayPositioning::RelativeToLine,
            }),
    );
}

fn extract_hover_markup(markup: &MarkedString) -> String {
    match markup {
        MarkedString::String(s) => s.clone(),
        MarkedString::LanguageString(LanguageString { language, value }) => {
            format!("```{language}\n{value}\n```")
        }
    }
}

pub async fn handle_hover(state: &State, msg: &JsonRpcMessage) {
    if let JsonRpcMessage::Response(response) = msg {
        let bufs = state.lock_state::<Buffers>().await;

        let mut buffer = None;
        for buf in &bufs.buffers {
            if let Some(state) = buf.read().await.get_state::<HoverState>().await
                && let Some(info) = &state.info
                && info.pending_request == response.id
            {
                // This is the right buffer!
                buffer = Some(buf.clone());
                break;
            }
        }

        let Some(buf) = buffer else {
            return;
        };

        let mut buf = buf.write_owned().await;
        let mut hover_state = buf.get_state_mut::<HoverState>().await.unwrap();
        let info = hover_state.info.as_mut().unwrap();

        if let Some(result) = &response.result {
            if let Ok(hover) = serde_json::from_value::<Hover>(result.clone()) {
                info.hover = Some(hover);
            } else {
                // Response was null or invalid, clear hover
                info.hover = None;
            }
        } else {
            // No hover information available
            info.hover = None;
        }
    }
}
