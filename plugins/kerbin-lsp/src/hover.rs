use std::sync::Arc;

use kerbin_core::*;
use lsp_types::{
    Hover, HoverContents, HoverParams, LanguageString, MarkedString, Position,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};
use ratatui::{
    layout::Rect,
    prelude::*,
    widgets::{Block, BorderType, Paragraph},
};

use crate::{JsonRpcMessage, LspManager, OpenedFile};
use kerbin_tree_sitter::{grammar_manager::GrammarManager, state::highlight_text};

pub struct HoverInfo {
    pub pending_request: i32,

    pub hover: Option<Hover>,

    pub position: usize,
    pub scroll_y: usize,
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
    #[command(drop_ident, name = "hover_scroll", name = "hs")]
    /// Scroll the hover documentation vertically
    Scroll { amount: isize },
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
                let cursor_byte = cursor.get_cursor_byte().min(buf.len());

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
                    scroll_y: 0,
                });
            }
            Self::Scroll { amount } => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut buf = bufs.cur_buffer_mut().await;

                if let Some(mut state) = buf.get_state_mut::<HoverState>().await
                    && let Some(info) = &mut state.info
                {
                    info.scroll_y = info.scroll_y.saturating_add_signed(*amount);
                }
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

    let Some(info) = state.info.as_mut() else {
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
        HoverContents::Scalar(t) => extract_hover_markup(t, &mut grammars, &config.0, &theme, &log),
        HoverContents::Array(a) => a
            .iter()
            .map(|x| extract_hover_markup(x, &mut grammars, &config.0, &theme, &log))
            .fold(vec![], |mut l, r| {
                l.push(("\n\n".to_string(), Style::default()));
                l.extend(r);
                l
            }),
        HoverContents::Markup(m) => {
            highlight_text(&m.value, "markdown", &mut grammars, &config.0, &theme, &log)
        }
    };

    const MAX_WIDTH: usize = 80;
    const MAX_HEIGHT: usize = 20;

    // Convert (String, Style) segments into wrapped Lines, splitting on newlines
    let mut all_lines: Vec<Vec<(char, Style)>> = Vec::new();
    let mut current_line: Vec<(char, Style)> = Vec::new();

    for (part, style) in text {
        for ch in part.chars() {
            if ch == '\n' {
                all_lines.push(std::mem::take(&mut current_line));
                continue;
            }
            if current_line.len() >= MAX_WIDTH {
                all_lines.push(std::mem::take(&mut current_line));
            }
            current_line.push((ch, style));
        }
    }
    if !current_line.is_empty() {
        all_lines.push(current_line);
    }

    if all_lines.is_empty() {
        return;
    }

    if info.scroll_y >= all_lines.len() {
        info.scroll_y = all_lines.len().saturating_sub(1);
    }
    let scroll_y = info.scroll_y;
    let height = all_lines.len().min(MAX_HEIGHT);

    let text_lines: Vec<Line<'static>> = all_lines
        .iter()
        .skip(scroll_y)
        .take(height)
        .map(|line| {
            Line::from(
                line.iter()
                    .map(|(ch, style)| Span::styled(ch.to_string(), *style))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    let popup_w = (MAX_WIDTH + 2) as u16;
    let popup_h = (height + 2) as u16;
    let popup_rect = Rect::new(0, 0, popup_w, popup_h);
    let mut popup_buf = ratatui::buffer::Buffer::empty(popup_rect);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title("Hover");
    let inner = block.inner(popup_rect);
    block.render(popup_rect, &mut popup_buf);
    Paragraph::new(Text::from(text_lines)).render(inner, &mut popup_buf);

    buf.add_extmark(
        ExtmarkBuilder::new("lsp::hover", info.position)
            .with_priority(5)
            .with_kind(ExtmarkKind::Overlay {
                content: Arc::new(popup_buf),
                offset_x: 0,
                offset_y: 1,
                z_index: 5,
            }),
    );
}

fn extract_hover_markup(
    markup: &MarkedString,
    grammars: &mut GrammarManager,
    config_path: &str,
    theme: &Theme,
    log: &LogSender,
) -> Vec<(String, Style)> {
    match markup {
        MarkedString::String(s) => highlight_text(s, "markdown", grammars, config_path, theme, log),
        MarkedString::LanguageString(LanguageString { language, value }) => {
            highlight_text(value, language, grammars, config_path, theme, log)
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
