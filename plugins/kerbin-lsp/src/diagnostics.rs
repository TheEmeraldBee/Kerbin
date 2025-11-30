use std::sync::Arc;

use crate::*;
use ascii_forge::prelude::*;
use kerbin_core::{ascii_forge::widgets::Border, *};
use lsp_types::*;

#[derive(Default, State)]
pub struct Diagnostics(pub Vec<Diagnostic>);

/// System that renders diagnostic highlights as extmarks
pub async fn render_diagnostic_highlights(buffers: ResMut<kerbin_core::Buffers>) {
    get!(mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    if let Some(diagnostics) = buf
        .get_state_mut::<Diagnostics>()
        .await
        .as_ref()
        .map(|x| &x.0)
    {
        // Clear old diagnostic extmarks
        buf.renderer.clear_extmark_ns("lsp::diagnostics");

        // Add new diagnostic highlights
        for diagnostic in diagnostics {
            let start_line = diagnostic.range.start.line as usize;
            let start_char = diagnostic.range.start.character as usize;
            let end_line = diagnostic.range.end.line as usize;
            let end_char = diagnostic.range.end.character as usize;

            // Convert line/char positions to byte offsets
            let start_byte = buf.rope.line_to_byte_idx(start_line, LineType::LF_CR)
                + buf.rope.char_to_byte_idx(start_char);

            let end_byte = buf.rope.line_to_byte_idx(end_line, LineType::LF_CR)
                + buf.rope.char_to_byte_idx(end_char);

            if (start_byte..end_byte).contains(&buf.primary_cursor().get_cursor_byte()) {
                let buffer = Buffer::sized_element(diagnostic.message.as_str().red());
                let mut render = Buffer::new(buffer.size() + vec2(2, 2));

                render!(render,
                    (0, 0) => [Border::rounded(buffer.size().x + 2, buffer.size().y + 2).with_title("Diagnostics".grey())],
                    (1, 1) => [buffer]
                );

                buf.add_extmark(
                    ExtmarkBuilder::new("lsp::diagnostics", start_byte)
                        .with_priority(5)
                        .with_decoration(ExtmarkDecoration::OverlayElement {
                            offset: vec2(1, 1),
                            elem: Arc::new(render),
                            z_index: 0,
                            clip_to_viewport: true,
                            positioning: OverlayPositioning::RelativeToLine,
                        }),
                );
            }

            // Choose color based on severity
            let style = match diagnostic.severity {
                Some(DiagnosticSeverity::ERROR) => ContentStyle::new().underline_red().underlined(),
                Some(DiagnosticSeverity::WARNING) => {
                    ContentStyle::new().underline_yellow().underlined()
                }
                Some(DiagnosticSeverity::INFORMATION) => {
                    ContentStyle::new().underline_blue().underlined()
                }
                Some(DiagnosticSeverity::HINT) => ContentStyle::new().underline_dark_green(),
                _ => ContentStyle::new().underline_red().underlined(),
            };

            buf.add_extmark(
                ExtmarkBuilder::new_range("lsp::diagnostics", start_byte..end_byte)
                    .with_priority(3)
                    .with_decoration(ExtmarkDecoration::Highlight { hl: style }),
            );
        }
    }
}

pub async fn publish_diagnostics(state: &State, msg: &JsonRpcMessage) {
    if let crate::JsonRpcMessage::Notification(notif) = msg
        && let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(notif.params.clone())
    {
        let Some(mut buf) = state
            .lock_state::<Buffers>()
            .await
            .get_mut_path(params.uri.path().as_str())
            .await
        else {
            return;
        };

        buf.set_state(Diagnostics(params.diagnostics));
    }
}
