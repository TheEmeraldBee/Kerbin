use crate::*;
use ascii_forge::prelude::*;
use kerbin_core::{kerbin_macros::State, *};
use lsp_types::*;
use std::collections::HashMap;

/// Stores diagnostics for each file
#[derive(Default, State)]
pub struct DiagnosticsState {
    /// Map from file URI to list of diagnostics
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
}

/// System that renders diagnostic highlights as extmarks
pub async fn render_diagnostic_highlights(
    buffers: ResMut<kerbin_core::Buffers>,
    diagnostics_state: Res<DiagnosticsState>,
) {
    get!(mut buffers, diagnostics_state);

    let mut buf = buffers.cur_buffer_mut().await;
    let file_uri = Uri::file_path(&buf.path).ok().map(|u| u.to_string());

    if let Some(uri) = file_uri
        && let Some(diagnostics) = diagnostics_state.diagnostics.get(&uri)
    {
        if !diagnostics.is_empty() {
            tracing::error!("{diagnostics:#?}");
        }

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

            // Choose color based on severity
            let style = match diagnostic.severity {
                Some(DiagnosticSeverity::ERROR) => ContentStyle::new().underline_red().underlined(),
                Some(DiagnosticSeverity::WARNING) => {
                    ContentStyle::new().underline_yellow().underlined()
                }
                Some(DiagnosticSeverity::INFORMATION) => {
                    ContentStyle::new().underline_blue().underlined()
                }
                Some(DiagnosticSeverity::HINT) => ContentStyle::new().grey(),
                _ => ContentStyle::new().underline_red().underlined(),
            };

            buf.renderer.add_extmark_range(
                "lsp::diagnostics",
                start_byte..end_byte,
                3,
                vec![ExtmarkDecoration::Highlight { hl: style }],
            );
        }
    }
}

pub async fn publish_diagnostics(state: &State, msg: &JsonRpcMessage) {
    if let crate::JsonRpcMessage::Notification(notif) = msg
        && let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(notif.params.clone())
    {
        let mut diagnostics_state = state.lock_state::<DiagnosticsState>().await;
        diagnostics_state
            .diagnostics
            .insert(params.uri.to_string(), params.diagnostics);
    }
}
