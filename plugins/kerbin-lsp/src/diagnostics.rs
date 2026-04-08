use std::sync::Arc;

use crate::*;
use lsp_types::*;

/// Global store of all diagnostics received via publishDiagnostics,
/// keyed by file path. Includes files not currently open as buffers.
#[derive(State, Default)]
pub struct GlobalDiagnostics(pub std::collections::HashMap<String, Vec<Diagnostic>>);

use ratatui::{
    layout::Rect,
    prelude::*,
    style::{Color, Modifier},
    widgets::{Block, BorderType, Paragraph},
};

struct DiagnosticWidget {
    message: String,
    style: Style,
}

impl DiagnosticWidget {
    fn popup_width(&self) -> u16 {
        (self.message.len() + 4).min(60) as u16
    }
}

impl OverlayWidget for DiagnosticWidget {
    fn dimensions(&self) -> (u16, u16) {
        (self.popup_width(), 3)
    }

    fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title("Diagnostics");
        let inner = block.inner(area);
        block.render(area, buf);
        let truncated: String = self.message.chars().take(inner.width as usize).collect();
        Paragraph::new(truncated).style(self.style).render(inner, buf);
    }
}

#[derive(Default, State)]
pub struct Diagnostics(pub Vec<Diagnostic>);

pub fn severity_to_str(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "information",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "error",
    }
}

pub fn format_diagnostic(path: &str, diag: &Diagnostic) -> String {
    let message = diag.message.replace('\n', " ");
    format!(
        "{}:{}:{}:{}:{}",
        path,
        diag.range.start.line + 1,
        diag.range.start.character + 1,
        severity_to_str(diag.severity),
        message,
    )
}

fn severity_to_style_ns(severity: Option<DiagnosticSeverity>) -> (Style, &'static str) {
    match severity {
        Some(DiagnosticSeverity::ERROR) | None => (
            Style::default()
                .underline_color(Color::Red)
                .add_modifier(Modifier::UNDERLINED),
            "lsp::diagnostics::error",
        ),
        Some(DiagnosticSeverity::WARNING) => (
            Style::default()
                .underline_color(Color::Yellow)
                .add_modifier(Modifier::UNDERLINED),
            "lsp::diagnostics::warning",
        ),
        Some(DiagnosticSeverity::INFORMATION) => (
            Style::default()
                .underline_color(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
            "lsp::diagnostics::info",
        ),
        Some(DiagnosticSeverity::HINT) => {
            (Style::default().underline_color(Color::DarkGray), "lsp::diagnostics::hint")
        }
        _ => (
            Style::default()
                .underline_color(Color::Red)
                .add_modifier(Modifier::UNDERLINED),
            "lsp::diagnostics::error",
        ),
    }
}

/// Returns a numeric severity rank for comparison (higher = more severe).
fn severity_rank(severity: Option<DiagnosticSeverity>) -> i32 {
    match severity {
        Some(DiagnosticSeverity::ERROR) | None => 3,
        Some(DiagnosticSeverity::WARNING) => 2,
        Some(DiagnosticSeverity::INFORMATION) => 1,
        _ => 0,
    }
}

/// System that renders diagnostic highlights as extmarks
pub async fn render_diagnostic_highlights(buffers: ResMut<kerbin_core::Buffers>) {
    get!(mut buffers);

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else { return; };

    let diagnostics: Vec<Diagnostic> = match buf.get_state_mut::<Diagnostics>().await.as_ref() {
        Some(d) => d.0.clone(),
        None => return,
    };

    // Takes buf explicitly to avoid capturing it in the closure (it's borrowed mutably below)
    let to_byte = |buf: &TextBuffer, line: usize, col: usize| -> usize {
        let total_lines = buf.len_lines();
        let line = line.min(total_lines.saturating_sub(1));

        let line_start_byte = buf.line_to_byte_clamped(line);
        let line_start_char = buf.byte_to_char_clamped(line_start_byte);

        let line_len_chars = buf.line_clamped(line).len_chars();
        let col = col.min(line_len_chars);

        let global_char = (line_start_char + col).min(buf.len_chars());

        buf.char_to_byte_clamped(global_char)
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte();

    // Register namespace priorities (idempotent).
    buf.renderer.set_namespace_priority("lsp::diagnostics::hint",    0);
    buf.renderer.set_namespace_priority("lsp::diagnostics::info",    1);
    buf.renderer.set_namespace_priority("lsp::diagnostics::warning", 2);
    buf.renderer.set_namespace_priority("lsp::diagnostics::error",   3);

    // Only show overlay for the highest-severity diagnostic at cursor.
    let popup_idx = diagnostics
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let start = to_byte(&buf, d.range.start.line as usize, d.range.start.character as usize);
            let end = to_byte(&buf, d.range.end.line as usize, d.range.end.character as usize);
            (start..end).contains(&cursor_byte).then_some((i, severity_rank(d.severity)))
        })
        .max_by_key(|(_, rank)| *rank)
        .map(|(i, _)| i);

    buf.renderer.clear_extmark_ns("lsp::diagnostics::hint");
    buf.renderer.clear_extmark_ns("lsp::diagnostics::info");
    buf.renderer.clear_extmark_ns("lsp::diagnostics::warning");
    buf.renderer.clear_extmark_ns("lsp::diagnostics::error");

    for (i, diagnostic) in diagnostics.iter().enumerate() {
        let start_byte = to_byte(
            &buf,
            diagnostic.range.start.line as usize,
            diagnostic.range.start.character as usize,
        );
        let end_byte = to_byte(
            &buf,
            diagnostic.range.end.line as usize,
            diagnostic.range.end.character as usize,
        );

        let (style, ns) = severity_to_style_ns(diagnostic.severity);

        if popup_idx == Some(i) {
            let widget = DiagnosticWidget {
                message: diagnostic.message.clone(),
                style,
            };
            buf.add_extmark(
                ExtmarkBuilder::new(ns, start_byte)
                    .with_kind(ExtmarkKind::Overlay {
                        widget: Arc::new(widget),
                        position: OverlayPosition::Fixed { offset_x: 0, offset_y: 1 },
                    }),
            );
        }

        buf.add_extmark(
            ExtmarkBuilder::new_range(ns, start_byte..end_byte)
                .with_kind(ExtmarkKind::Highlight { style }),
        );
    }
}

pub async fn publish_diagnostics(state: &State, msg: &JsonRpcMessage) {
    if let crate::JsonRpcMessage::Notification(notif) = msg
        && let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(notif.params.clone())
    {
        let path = params.uri.path().to_string();

        // Always store in the global map so workspace diagnostics work for
        // files that are not currently open as buffers.
        state
            .lock_state::<GlobalDiagnostics>()
            .await
            .0
            .insert(path.clone(), params.diagnostics.clone());

        // Also push onto the open buffer if there is one.
        if let Some(mut buf_guard) = state
            .lock_state::<Buffers>()
            .await
            .get_mut_path(&path)
            .await
            && let Some(buf) = buf_guard.downcast_mut::<TextBuffer>() {
                buf.set_state(Diagnostics(params.diagnostics));
            }
    }
}

