use std::sync::Arc;

use crate::*;
use kerbin_core::*;
use lsp_types::*;
use ratatui::{
    layout::Rect,
    prelude::*,
    style::{Color, Modifier},
    widgets::{Block, BorderType, Paragraph},
};

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

fn severity_to_style_priority(severity: Option<DiagnosticSeverity>) -> (Style, i32) {
    match severity {
        Some(DiagnosticSeverity::ERROR) => (
            Style::default()
                .underline_color(Color::Red)
                .add_modifier(Modifier::UNDERLINED),
            3,
        ),
        Some(DiagnosticSeverity::WARNING) => (
            Style::default()
                .underline_color(Color::Yellow)
                .add_modifier(Modifier::UNDERLINED),
            2,
        ),
        Some(DiagnosticSeverity::INFORMATION) => (
            Style::default()
                .underline_color(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
            1,
        ),
        Some(DiagnosticSeverity::HINT) => (Style::default().underline_color(Color::DarkGray), 0),
        _ => (
            Style::default()
                .underline_color(Color::Red)
                .add_modifier(Modifier::UNDERLINED),
            3,
        ),
    }
}

/// System that renders diagnostic highlights as extmarks
pub async fn render_diagnostic_highlights(buffers: ResMut<kerbin_core::Buffers>) {
    get!(mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    // Clone diagnostics so buf is free for mutable use afterward
    let diagnostics: Vec<Diagnostic> = match buf.get_state_mut::<Diagnostics>().await.as_ref() {
        Some(d) => d.0.clone(),
        None => return,
    };

    // Helper to safe-convert (line, col) -> byte index; takes buf explicitly to avoid capturing
    let to_byte = |buf: &TextBuffer, line: usize, col: usize| -> usize {
        let total_lines = buf.len_lines();
        let line = line.min(total_lines.saturating_sub(1));

        let line_start_byte = buf.line_to_byte_clamped(line);
        let line_start_char = buf.byte_to_char_clamped(line_start_byte);

        let line_len_chars = buf.line_clamped(line).len_chars();

        // Clamp col to line length to avoid crossing into next line
        let col = col.min(line_len_chars);

        let global_char = line_start_char + col;
        // Clamp to total chars
        let global_char = global_char.min(buf.len_chars());

        buf.char_to_byte_clamped(global_char)
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte();

    // Find the index of the highest-priority diagnostic whose range contains the cursor.
    // Only this one will get a popup overlay.
    let popup_idx = diagnostics
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let start = to_byte(&buf, d.range.start.line as usize, d.range.start.character as usize);
            let end = to_byte(&buf, d.range.end.line as usize, d.range.end.character as usize);
            let (_, prio) = severity_to_style_priority(d.severity);
            (start..end).contains(&cursor_byte).then_some((i, prio))
        })
        .max_by_key(|(_, prio)| *prio)
        .map(|(i, _)| i);

    // Clear old diagnostic extmarks
    buf.renderer.clear_extmark_ns("lsp::diagnostics");

    // Add new diagnostic highlights
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

        let (style, priority) = severity_to_style_priority(diagnostic.severity);

        // Show floating diagnostic message only for the single highest-priority active diagnostic
        if popup_idx == Some(i) {
            let msg = diagnostic.message.as_str();
            let popup_w = (msg.len() + 4).min(60) as u16;
            let popup_h = 3u16;
            let popup_rect = Rect::new(0, 0, popup_w, popup_h);
            let mut popup_buf = ratatui::buffer::Buffer::empty(popup_rect);

            let block = Block::bordered()
                .border_type(BorderType::Rounded)
                .title("Diagnostics");
            let inner = block.inner(popup_rect);
            block.render(popup_rect, &mut popup_buf);

            let truncated: String = msg.chars().take(inner.width as usize).collect();
            Paragraph::new(truncated)
                .style(style)
                .render(inner, &mut popup_buf);

            buf.add_extmark(
                ExtmarkBuilder::new("lsp::diagnostics", start_byte)
                    .with_priority(priority)
                    .with_kind(ExtmarkKind::Overlay {
                        content: Arc::new(popup_buf),
                        offset_x: 0,
                        offset_y: 1,
                        z_index: priority,
                    }),
            );
        }

        buf.add_extmark(
            ExtmarkBuilder::new_range("lsp::diagnostics", start_byte..end_byte)
                .with_priority(priority)
                .with_kind(ExtmarkKind::Highlight { style }),
        );
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
