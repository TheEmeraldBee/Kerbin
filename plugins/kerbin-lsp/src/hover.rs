use std::sync::Arc;

use kerbin_core::{
    ascii_forge::{prelude::*, widgets::Border},
    kerbin_macros::{Command, State},
    *,
};
use lsp_types::{
    Hover, HoverContents, HoverParams, MarkedString, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Uri, WorkDoneProgressParams,
};

use crate::{JsonRpcMessage, LspManager, OpenedFiles, UriExt};

#[derive(Default, State)]
pub struct HoverState {
    pub hover: Option<Hover>,

    pub position: Option<usize>,

    pub pending_request: Option<i32>,
}

#[derive(Command)]
pub enum HoverCommand {
    /// Display the hover at the current position of the editor
    #[command]
    Hover,
}

#[async_trait::async_trait]
impl Command for HoverCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut lsp_manager = state.lock_state::<LspManager>().await.unwrap();
        let mut hover_state = state.lock_state::<HoverState>().await.unwrap();
        let mut opened_files = state.lock_state::<OpenedFiles>().await.unwrap();
        let buf = state
            .lock_state::<Buffers>()
            .await
            .unwrap()
            .cur_buffer()
            .await;

        let file_path = buf.path.clone();

        let Some(file) = opened_files.opened.get_mut(&file_path) else {
            return false;
        };

        // Get the LSP client for this language
        let Some(client) = lsp_manager.get_or_create_client(&file.lang).await else {
            return false;
        };

        let cursor = buf.primary_cursor();

        let cursor_byte = cursor.get_cursor_byte();

        let line = buf.rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);
        let character = cursor_byte - buf.rope.line_to_byte_idx(line, LineType::LF_CR);

        // Create hover request parameters
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Uri::file_path(&buf.path).unwrap(),
                },
                position: Position::new(line as u32, character as u32),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        // Send the hover request
        match client.request("textDocument/hover", params).await {
            Ok(id) => {
                hover_state.pending_request = Some(id);
                hover_state.position = Some(cursor_byte);
                true
            }
            Err(_) => false,
        }
    }
}

pub async fn render_hover(buffers: ResMut<Buffers>, hover_state: Res<HoverState>) {
    get!(mut buffers, hover_state);

    let mut buf = buffers.cur_buffer_mut().await;

    buf.renderer.clear_extmark_ns("lsp::hover");

    if let Some(hover) = &hover_state.hover
        && let Some(byte) = hover_state.position
    {
        // Extract hover content
        let content = match &hover.contents {
            HoverContents::Scalar(markup) => format_markup_content(markup),
            HoverContents::Array(markups) => markups
                .iter()
                .map(format_markup_content)
                .collect::<Vec<_>>()
                .join("\n"),
            HoverContents::Markup(markup) => markup.value.clone(),
        };

        let content = Buffer::sized_element(content);

        let mut elem = Buffer::new(content.size() + vec2(2, 2));

        let border = Border::rounded(elem.size().x, elem.size().y);
        render!(elem, (0, 0) => [ border ]);

        render!(elem, (1, 1) => [ content ]);

        buf.renderer.add_extmark(
            "lsp::hover",
            byte,
            5,
            vec![ExtmarkDecoration::OverlayElement {
                offset: vec2(0, 0),
                elem: Arc::new(elem),
                z_index: 1,
                clip_to_viewport: true,
                positioning: OverlayPositioning::RelativeToChar,
            }],
        );
    }
}

pub async fn handle_hover(state: &State, msg: &JsonRpcMessage) {
    if let JsonRpcMessage::Response(response) = msg {
        let mut hover_state = state.lock_state::<HoverState>().await.unwrap();

        // Check if this response is for our pending hover request
        if hover_state.pending_request == Some(response.id) {
            hover_state.pending_request = None;

            if let Some(result) = &response.result {
                // Try to parse the hover response
                if let Ok(hover) = serde_json::from_value::<Hover>(result.clone()) {
                    hover_state.hover = Some(hover);
                } else {
                    // Response was null or invalid, clear hover
                    hover_state.hover = None;
                }
            } else {
                // No hover information available
                hover_state.hover = None;
            }
        }
    }
}

pub async fn clear_hover_on_move(
    buffers: ResMut<kerbin_core::Buffers>,
    hover_state: ResMut<HoverState>,
) {
    get!(buffers, mut hover_state);

    let buf = buffers.cur_buffer().await;
    let cursor = buf.primary_cursor().get_cursor_byte();

    // If cursor moved, clear hover
    if let Some(pos) = hover_state.position {
        if cursor != pos {
            hover_state.hover = None;
            hover_state.position = None;
        }
    }
}

fn format_markup_content(content: &MarkedString) -> String {
    match content {
        MarkedString::String(s) => s.clone(),
        MarkedString::LanguageString(ls) => {
            // Format as code block with language
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
    }
}
