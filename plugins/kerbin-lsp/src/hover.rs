use std::collections::HashMap;
use std::sync::Arc;

use kerbin_core::{
    ascii_forge::{prelude::*, widgets::Border},
    kerbin_macros::{Command, State},
    *,
};
use lsp_types::{
    Hover, HoverContents, HoverParams, MarkedString, MarkupKind, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};

use crate::{JsonRpcMessage, LspManager, OpenedFiles};

pub use kerbin_tree_sitter::highlight_string::StyledLine;

/// Per-buffer hover information
#[derive(Clone)]
pub struct BufferHoverInfo {
    pub hover: Option<Hover>,
    pub position: Option<usize>,
    pub scroll_offset: usize,
    pub total_lines: usize,
}

impl Default for BufferHoverInfo {
    fn default() -> Self {
        Self {
            hover: None,
            position: None,
            scroll_offset: 0,
            total_lines: 0,
        }
    }
}

#[derive(Default, State)]
pub struct HoverState {
    /// Map from buffer path to hover info
    pub buffer_hovers: HashMap<String, BufferHoverInfo>,
    /// Map from request ID to buffer path
    pub pending_requests: HashMap<i32, String>,
}

#[derive(Command)]
pub enum HoverCommand {
    /// Display the hover at the current position of the editor
    #[command]
    Hover,

    /// Scroll the hover window up by the given number of lines
    #[command(drop_ident, name = "hover_scroll_up", name = "hsu")]
    ScrollUp(#[command(type_name = "usize?")] Option<usize>),

    /// Scroll the hover window down by the given number of lines
    #[command(drop_ident, name = "hover_scroll_down", name = "hsd")]
    ScrollDown(#[command(type_name = "usize?")] Option<usize>),

    /// Scroll to the top of the hover window
    #[command(drop_ident, name = "hover_scroll_top", name = "hst")]
    ScrollTop,

    /// Scroll to the bottom of the hover window
    #[command(drop_ident, name = "hover_scroll_bottom", name = "hsb")]
    ScrollBottom,
}

#[async_trait::async_trait]
impl Command for HoverCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Hover => {
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
                            uri: file.uri.clone(),
                        },
                        position: Position::new(line as u32, character as u32),
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                };

                // Send the hover request
                match client.request("textDocument/hover", params).await {
                    Ok(id) => {
                        // Store which buffer this request is for
                        hover_state.pending_requests.insert(id, file_path.clone());

                        // Get or create buffer hover info
                        let info = hover_state.buffer_hovers.entry(file_path).or_default();
                        info.position = Some(cursor_byte);
                        info.scroll_offset = 0;

                        true
                    }
                    Err(_) => false,
                }
            }

            Self::ScrollUp(amount) => {
                let mut hover_state = state.lock_state::<HoverState>().await.unwrap();
                let buffers = state.lock_state::<Buffers>().await.unwrap();
                let buf = buffers.cur_buffer().await;
                let file_path = buf.path.clone();

                // Get hover info for current buffer
                let Some(info) = hover_state.buffer_hovers.get_mut(&file_path) else {
                    return false;
                };

                // Only scroll if hover is active
                if info.hover.is_none() {
                    return false;
                }

                let scroll_amount = amount.unwrap_or(1);
                info.scroll_offset = info.scroll_offset.saturating_sub(scroll_amount);
                true
            }

            Self::ScrollDown(amount) => {
                let mut hover_state = state.lock_state::<HoverState>().await.unwrap();
                let buffers = state.lock_state::<Buffers>().await.unwrap();
                let buf = buffers.cur_buffer().await;
                let file_path = buf.path.clone();

                // Get hover info for current buffer
                let Some(info) = hover_state.buffer_hovers.get_mut(&file_path) else {
                    return false;
                };

                // Only scroll if hover is active
                if info.hover.is_none() {
                    return false;
                }

                let scroll_amount = amount.unwrap_or(1);
                let max_scroll = info.total_lines.saturating_sub(1);
                info.scroll_offset = (info.scroll_offset + scroll_amount).min(max_scroll);
                true
            }

            Self::ScrollTop => {
                let mut hover_state = state.lock_state::<HoverState>().await.unwrap();
                let buffers = state.lock_state::<Buffers>().await.unwrap();
                let buf = buffers.cur_buffer().await;
                let file_path = buf.path.clone();

                let Some(info) = hover_state.buffer_hovers.get_mut(&file_path) else {
                    return false;
                };

                if info.hover.is_none() {
                    return false;
                }

                info.scroll_offset = 0;
                true
            }

            Self::ScrollBottom => {
                let mut hover_state = state.lock_state::<HoverState>().await.unwrap();
                let buffers = state.lock_state::<Buffers>().await.unwrap();
                let buf = buffers.cur_buffer().await;
                let file_path = buf.path.clone();

                let Some(info) = hover_state.buffer_hovers.get_mut(&file_path) else {
                    return false;
                };

                if info.hover.is_none() {
                    return false;
                }

                info.scroll_offset = info.total_lines.saturating_sub(1);
                true
            }
        }
    }
}

pub async fn render_hover(
    buffers: ResMut<Buffers>,
    hover_state: ResMut<HoverState>,
    theme: Res<Theme>,
    grammars: ResMut<kerbin_tree_sitter::GrammarManager>,
) {
    get!(mut buffers, mut hover_state, theme, mut grammars);

    let mut buf = buffers.cur_buffer_mut().await;
    let file_path = buf.path.clone();

    buf.renderer.clear_extmark_ns("lsp::hover");

    // Get hover info for current buffer
    let Some(info) = hover_state.buffer_hovers.get_mut(&file_path) else {
        return;
    };

    let lang = grammars
        .extension_map
        .get(&buf.ext)
        .cloned()
        .unwrap_or("plaintext".to_string());

    if let Some(hover) = &info.hover
        && let Some(byte) = info.position
    {
        // Extract hover content and convert to markdown
        let markdown_content = extract_hover_as_markdown(&hover.contents);

        // Use tree-sitter to highlight the markdown
        let content_lines = kerbin_tree_sitter::highlight_string::highlight_markdown(
            &markdown_content,
            &lang,
            &mut grammars,
            &theme,
        );

        // Store total lines for scroll bounds checking
        info.total_lines = content_lines.len();

        let elem = create_hover_buffer(&content_lines, &theme, info.scroll_offset);

        buf.renderer.add_extmark(
            "lsp::hover",
            byte,
            5,
            vec![ExtmarkDecoration::OverlayElement {
                offset: vec2(0, 1),
                elem: Arc::new(elem),
                z_index: 1,
                clip_to_viewport: true,
                positioning: OverlayPositioning::RelativeToChar,
            }],
        );
    }
}

/// Extract hover content and convert to markdown format
fn extract_hover_as_markdown(contents: &HoverContents) -> String {
    match contents {
        HoverContents::Scalar(markup) => format_markup_to_markdown(markup),
        HoverContents::Array(markups) => {
            markups
                .iter()
                .map(format_markup_to_markdown)
                .collect::<Vec<_>>()
                .join("\n\n---\n\n") // Separator between multiple hovers
        }
        HoverContents::Markup(markup) => match markup.kind {
            MarkupKind::Markdown => markup.value.clone(),
            MarkupKind::PlainText => markup.value.clone(),
        },
    }
}

/// Convert a MarkedString to markdown
fn format_markup_to_markdown(markup: &MarkedString) -> String {
    match markup {
        MarkedString::String(s) => s.clone(),
        MarkedString::LanguageString(ls) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
    }
}

fn wrap_styled_lines(lines: &[StyledLine], max_width: usize) -> Vec<StyledLine> {
    let mut wrapped_lines = Vec::new();

    for line in lines {
        if line.is_empty() {
            wrapped_lines.push(StyledLine::new());
            continue;
        }

        let is_whitespace_only = line.segments.iter().all(|(text, _)| text.trim().is_empty());
        if is_whitespace_only {
            wrapped_lines.push(StyledLine::new());
            continue;
        }

        let mut current_line = StyledLine::new();
        let mut current_width = 0;

        for (text, style) in &line.segments {
            if text.is_empty() {
                continue;
            }

            let mut remaining_text = text.as_str();

            while !remaining_text.is_empty() {
                let is_whitespace_unit = remaining_text.chars().next().unwrap().is_whitespace();

                let (unit, unit_len) = if is_whitespace_unit {
                    let first_non_space = remaining_text
                        .find(|c: char| !c.is_whitespace())
                        .unwrap_or(remaining_text.len());
                    let unit = &remaining_text[..first_non_space];
                    (unit, unit.len())
                } else {
                    let first_space = remaining_text
                        .find(|c: char| c.is_whitespace())
                        .unwrap_or(remaining_text.len());
                    let unit = &remaining_text[..first_space];
                    (unit, unit.len())
                };

                if !is_whitespace_unit {
                    if unit_len > max_width {
                        if current_width > 0 {
                            wrapped_lines.push(current_line);
                            current_line = StyledLine::new();
                            current_width = 0;
                        }

                        let chars: Vec<char> = unit.chars().collect();
                        let mut chunk_start = 0;
                        while chunk_start < chars.len() {
                            let chunk_end = (chunk_start + max_width).min(chars.len());
                            let chunk: String = chars[chunk_start..chunk_end].iter().collect();

                            current_line.push(chunk.clone(), *style);
                            chunk_start = chunk_end;

                            if chunk_start < chars.len() {
                                wrapped_lines.push(current_line);
                                current_line = StyledLine::new();
                                current_width = 0;
                            } else {
                                current_width = chunk.len();
                            }
                        }
                    } else if current_width + unit_len > max_width && current_width > 0 {
                        wrapped_lines.push(current_line);
                        current_line = StyledLine::new();

                        current_line.push(unit.to_string(), *style);
                        current_width = unit_len;
                    } else {
                        current_line.push(unit.to_string(), *style);
                        current_width += unit_len;
                    }
                } else {
                    // --- Whitespace Handling ---
                    let mut space_idx = 0;
                    let chars: Vec<char> = unit.chars().collect();

                    while space_idx < chars.len() {
                        let available = max_width - current_width;
                        let chunk_size = chars[space_idx..].len().min(available);

                        if chunk_size == 0 && current_width > 0 {
                            wrapped_lines.push(current_line);
                            current_line = StyledLine::new();
                            current_width = 0;
                            continue;
                        }

                        let chunk: String =
                            chars[space_idx..space_idx + chunk_size].iter().collect();
                        let chunk_len = chunk.len();

                        current_line.push(chunk, *style);
                        current_width += chunk_len;
                        space_idx += chunk_size;
                    }
                }

                remaining_text = &remaining_text[unit_len..];
            }
        }

        if !current_line.is_empty() {
            wrapped_lines.push(current_line);
        }
    }

    wrapped_lines
}

/// Create a hover buffer from styled lines with scrolling support
fn create_hover_buffer(lines: &[StyledLine], theme: &Theme, scroll_offset: usize) -> Buffer {
    if lines.is_empty() {
        return Buffer::new((10, 3));
    }

    // Fixed width for hover windows (configurable)
    const HOVER_MAX_WIDTH: usize = 60;
    const MAX_VISIBLE_LINES: usize = 20;

    // Wrap lines to fit within max width
    let wrapped_lines = wrap_styled_lines(lines, HOVER_MAX_WIDTH);

    // Calculate actual width needed (up to max)
    let actual_width = wrapped_lines
        .iter()
        .map(|l| l.width())
        .max()
        .unwrap_or(10)
        .min(HOVER_MAX_WIDTH)
        .max(20);

    let visible_lines = wrapped_lines.len().min(MAX_VISIBLE_LINES);
    let height = visible_lines;

    // Add padding for border
    let mut elem = Buffer::new(((actual_width + 4) as u16, (height + 2) as u16));

    let border_style = theme.get_fallback_default(["ui.commandline.border", "ui.text"]);
    let mut border = Border::rounded(elem.size().x, elem.size().y);
    border.style = border_style;

    render!(elem, (0, 0) => [border]);

    // Calculate scroll position
    let start_line = scroll_offset.min(wrapped_lines.len().saturating_sub(1));
    let end_line = (start_line + visible_lines).min(wrapped_lines.len());

    // Render visible lines with scroll offset applied
    for (display_y, line_idx) in (start_line..end_line).enumerate() {
        let line = &wrapped_lines[line_idx];
        let mut x = 2u16;
        for (text, style) in &line.segments {
            render!(elem, (x, display_y as u16 + 1) => [style.apply(text)]);
            x += text.len() as u16;
        }
    }

    // Add scroll indicator if there's more content
    let scroll_indicator_style = theme.get_fallback_default(["ui.commandline.icon", "ui.text"]);

    if scroll_offset > 0 {
        // Show up arrow indicating more content above
        render!(elem, (elem.size().x - 2, 0) => [scroll_indicator_style.apply("▲")]);
    }

    if end_line < wrapped_lines.len() {
        // Show down arrow indicating more content below
        render!(elem, (elem.size().x - 2, elem.size().y - 1) => [scroll_indicator_style.apply("▼")]);
    }

    elem
}

pub async fn handle_hover(state: &State, msg: &JsonRpcMessage) {
    if let JsonRpcMessage::Response(response) = msg {
        let mut hover_state = state.lock_state::<HoverState>().await.unwrap();

        // Check if this response is for one of our pending hover requests
        if let Some(file_path) = hover_state.pending_requests.remove(&response.id) {
            // Get or create hover info for this buffer
            let info = hover_state.buffer_hovers.entry(file_path).or_default();

            if let Some(result) = &response.result {
                // Try to parse the hover response
                if let Ok(hover) = serde_json::from_value::<Hover>(result.clone()) {
                    info.hover = Some(hover);
                    // Reset scroll when new hover content arrives
                    info.scroll_offset = 0;
                } else {
                    // Response was null or invalid, clear hover
                    info.hover = None;
                    info.scroll_offset = 0;
                    info.total_lines = 0;
                }
            } else {
                // No hover information available
                info.hover = None;
                info.scroll_offset = 0;
                info.total_lines = 0;
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

    let Some(hover) = hover_state.buffer_hovers.get_mut(&buf.path) else {
        return;
    };

    // If cursor moved, clear hover and reset scroll
    if let Some(pos) = hover.position {
        if cursor != pos {
            hover.hover = None;
            hover.position = None;
            hover.scroll_offset = 0;
            hover.total_lines = 0;
        }
    }
}
