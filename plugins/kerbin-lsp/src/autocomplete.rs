use std::sync::Arc;

use kerbin_core::{
    TextBuffer,
    ascii_forge::{prelude::*, widgets::Border},
    theme::Theme,
    *,
};
use lsp_types::{
    CompletionItem, CompletionParams, CompletionResponse, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};

use ropey::RopeSlice;

use crate::{JsonRpcMessage, LspManager, OpenedFile};
use kerbin_tree_sitter::{grammar_manager::GrammarManager, state::highlight_text};

pub struct CompletionInfo {
    pub pending_request: i32,
    pub items: Vec<CompletionItem>,
    pub position: usize,
    pub selected_index: usize,
    pub cached_doc_buffer: Option<(usize, Arc<Buffer>)>,
}

#[derive(State, Default)]
pub struct CompletionState {
    pub info: Option<CompletionInfo>,
    pub just_accepted: bool,
}

#[derive(Command)]
pub enum CompletionCommand {
    #[command(drop_ident, name = "start_lsp_autocomplete", name = "sla")]
    /// Start requesting completions
    StartRequest,
    #[command(drop_ident, name = "accept_lsp_autocomplete", name = "ala")]
    /// Accept the currently selected completion
    Accept,
    #[command(drop_ident, name = "trash_lsp_autocomplete", name = "tla")]
    /// Trash the current completion request
    Trash,
    #[command(drop_ident, name = "select_next_lsp_completion", name = "snlc")]
    /// Select the next completion item
    SelectNext,
    #[command(drop_ident, name = "select_prev_lsp_completion", name = "splc")]
    /// Select the previous completion item
    SelectPrevious,
}

async fn trigger_completion_request(buf: &mut TextBuffer, lsps: &mut LspManager) -> Option<i32> {
    let file = buf.get_state::<OpenedFile>().await?;

    let client = lsps.get_or_create_client(&file.lang).await?;

    let cursor = buf.primary_cursor();
    let cursor_byte = cursor.get_cursor_byte().min(buf.len());

    let line = buf.byte_to_line_clamped(cursor_byte);
    let character = cursor_byte - buf.line_to_byte_clamped(line);

    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: file.uri.clone(),
            },
            position: Position::new(line as u32, character as u32),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
        context: None,
    };

    client.request("textDocument/completion", params).await.ok()
}

fn get_ranked_items<'a>(
    items: &'a [CompletionItem],
    query: &str,
) -> Vec<(&'a CompletionItem, i32)> {
    if query.is_empty() {
        return items.iter().map(|item| (item, 0)).collect();
    }

    #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
    enum MatchQuality {
        Fuzzy,
        Prefix,
        Exact,
    }

    let mut matched_items: Vec<(&CompletionItem, MatchQuality, i32)> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let text = item.filter_text.as_deref().unwrap_or(&item.label);

            let quality = if text == query {
                MatchQuality::Exact
            } else if text.starts_with(query) {
                MatchQuality::Prefix
            } else if kerbin_core::palette::ranking::rank(query, text).is_some() {
                MatchQuality::Fuzzy
            } else {
                return None;
            };

            Some((item, quality, idx as i32))
        })
        .collect();

    matched_items.sort_by(|(item_a, quality_a, _), (item_b, quality_b, _)| {
        match (&item_a.sort_text, &item_b.sort_text) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => {
                // Fallback to match quality (Higher is better)
                quality_b.cmp(quality_a)
            }
        }
    });

    matched_items
        .into_iter()
        .map(|(item, _, _)| (item, 0)) // Score is unused by callers
        .collect()
}

#[async_trait::async_trait]
impl Command for CompletionCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::StartRequest => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut lsps = state.lock_state::<LspManager>().await;

                let mut buf = bufs.cur_buffer_mut().await;

                let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());

                // Check if there's already an active completion
                let mut state = buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(existing_info) = &state.info {
                    let pos = existing_info.position.min(buf.len());
                    // Check if cursor is out of range of current completion
                    let start_line = buf.byte_to_line_clamped(pos);
                    let current_line = buf.byte_to_line_clamped(cursor_byte);

                    if cursor_byte < pos || start_line != current_line {
                        // Cancel the old completion if cursor moved out of range
                        state.info = None;
                    } else {
                        // There's already an active completion in valid range, don't start a new one
                        return true;
                    }
                }

                if let Some(id) = trigger_completion_request(&mut buf, &mut lsps).await {
                    let mut start_pos = cursor_byte;
                    let cursor_char_idx = buf.byte_to_char_clamped(cursor_byte);

                    let mut current_char_idx = cursor_char_idx;
                    while current_char_idx > 0 {
                        let prev_char = buf.char_clamped(current_char_idx - 1);
                        if !prev_char.is_alphanumeric() && prev_char != '_' {
                            break;
                        }
                        current_char_idx -= 1;
                    }

                    // 1-char requirement check
                    if cursor_char_idx <= current_char_idx {
                        return true;
                    }

                    if current_char_idx < cursor_char_idx {
                        start_pos = buf.char_to_byte_clamped(current_char_idx);
                    }

                    state.info = Some(CompletionInfo {
                        pending_request: id,
                        items: vec![],
                        position: start_pos,
                        selected_index: 0,
                        cached_doc_buffer: None,
                    });
                }
            }
            Self::Accept => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut buf = bufs.cur_buffer_mut().await;

                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(info) = &completion_state.info {
                    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
                    let pos = info.position.min(buf.len());

                    let start_line = buf.byte_to_line_clamped(pos);
                    let current_line = buf.byte_to_line_clamped(cursor_byte);

                    if start_line == current_line {
                        let query = if cursor_byte >= pos {
                            buf.slice_to_string(pos, cursor_byte).unwrap_or_default()
                        } else {
                            String::new()
                        };

                        // Use the exact same ranking logic as render
                        let ranked_items = get_ranked_items(&info.items, &query);

                        if !ranked_items.is_empty() {
                            let idx = info.selected_index % ranked_items.len();
                            // Get the item from ranked items using selected_index
                            if let Some((item, _)) = ranked_items.get(idx) {
                                let (start_byte, end_byte, text) =
                                    if let Some(lsp_types::CompletionTextEdit::Edit(e)) =
                                        &item.text_edit
                                    {
                                        // Helper to calculate line content length excluding line endings
                                        let line_content_len = |line_slice: &RopeSlice| {
                                            let mut len = line_slice.len_chars();
                                            if len > 0 {
                                                match line_slice.char(len - 1) {
                                                    '\n' => {
                                                        len -= 1;
                                                        if len > 0
                                                            && line_slice.char(len - 1) == '\r'
                                                        {
                                                            len -= 1;
                                                        }
                                                    }
                                                    '\r' => len -= 1,
                                                    _ => {}
                                                }
                                            }
                                            len
                                        };

                                        let max_line = buf.len_lines().saturating_sub(1);
                                        let start_line =
                                            (e.range.start.line as usize).min(max_line);
                                        let end_line = (e.range.end.line as usize).min(max_line);

                                        let start_line_slice = buf.line_clamped(start_line);
                                        let start_char = (e.range.start.character as usize)
                                            .min(line_content_len(&start_line_slice));
                                        let start = buf.line_to_byte_clamped(start_line)
                                            + start_line_slice.char_to_byte_idx(start_char);

                                        let end_line_slice = buf.line_clamped(end_line);
                                        let end_char = (e.range.end.character as usize)
                                            .min(line_content_len(&end_line_slice));
                                        let end = buf.line_to_byte_clamped(end_line)
                                            + end_line_slice.char_to_byte_idx(end_char);

                                        (start, end, e.new_text.clone())
                                    } else {
                                        (
                                            info.position,
                                            cursor_byte,
                                            item.insert_text
                                                .as_ref()
                                                .unwrap_or(&item.label)
                                                .clone(),
                                        )
                                    };

                                // Delete old text if needed
                                if end_byte > start_byte {
                                    let len_chars = buf.byte_to_char_clamped(end_byte)
                                        - buf.byte_to_char_clamped(start_byte);
                                    buf.action(kerbin_core::buffer::action::Delete {
                                        byte: start_byte,
                                        len: len_chars,
                                    });
                                }

                                // Insert new text
                                buf.action(kerbin_core::buffer::action::Insert {
                                    byte: start_byte,
                                    content: text.clone(),
                                });

                                // Move cursor to end of inserted text
                                buf.primary_cursor_mut()
                                    .set_sel(start_byte + text.len()..=start_byte + text.len());
                            }
                        }
                    }
                }

                completion_state.info = None;
                completion_state.just_accepted = true;
                resolver_engine_mut().await.trash_template("lsp_items");
            }
            Self::Trash => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut buf = bufs.cur_buffer_mut().await;

                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                // Clear completion state
                completion_state.info = None;
                resolver_engine_mut().await.trash_template("lsp_items");
            }
            Self::SelectNext => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut buf = bufs.cur_buffer_mut().await;
                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(info) = &mut completion_state.info {
                    info.selected_index += 1;
                }
            }
            Self::SelectPrevious => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut buf = bufs.cur_buffer_mut().await;
                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(info) = &mut completion_state.info
                    && info.selected_index > 0
                {
                    info.selected_index -= 1;
                }
            }
        }

        true
    }
}

pub async fn handle_completion(state: &State, msg: &JsonRpcMessage) {
    if let JsonRpcMessage::Response(response) = msg {
        let bufs = state.lock_state::<Buffers>().await;

        let mut buffer = None;
        for buf in &bufs.buffers {
            if let Some(state) = buf.read().await.get_state::<CompletionState>().await
                && let Some(info) = &state.info
                && info.pending_request == response.id
            {
                buffer = Some(buf.clone());
                break;
            }
        }

        let Some(buf) = buffer else {
            return;
        };

        let mut buf = buf.write_owned().await;
        let mut completion_state = buf.get_state_mut::<CompletionState>().await.unwrap();
        let info = completion_state.info.as_mut().unwrap();

        if let Some(result) = &response.result {
            if let Ok(response) = serde_json::from_value::<CompletionResponse>(result.clone()) {
                match response {
                    CompletionResponse::Array(items) => {
                        info.items = items;
                    }
                    CompletionResponse::List(list) => {
                        info.items = list.items;
                    }
                }
            } else {
                info.items = vec![];
            }
        } else {
            info.items = vec![];
        }

        info.selected_index = 0;

        let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
        let pos = info.position.min(buf.len());

        let query = if cursor_byte >= pos {
            buf.slice_to_string(pos, cursor_byte).unwrap_or_default()
        } else {
            String::new()
        };

        let items: Vec<String> = info
            .items
            .iter()
            .filter(|i| kerbin_core::palette::ranking::rank(&query, &i.label).is_some())
            .map(|i| i.label.clone())
            .collect();

        resolver_engine_mut().await.set_template("lsp_items", items);
    }
}

pub async fn update_completions(bufs: ResMut<Buffers>, lsps: ResMut<LspManager>) {
    get!(mut bufs, mut lsps);

    let mut buf = bufs.cur_buffer_mut().await;

    if buf.byte_changes.is_empty() {
        return;
    }

    if let Some(mut state) = buf.get_state_mut::<CompletionState>().await
        && state.just_accepted
    {
        state.just_accepted = false;
        // Clear info on acceptance update to be safe
        state.info = None;
        resolver_engine_mut().await.trash_template("lsp_items");
        return;
    }

    // Check criteria: at least 1 real non-whitespace character before cursor
    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
    let cursor_char_idx = buf.byte_to_char_clamped(cursor_byte);

    let mut current_char_idx = cursor_char_idx;
    while current_char_idx > 0 {
        let prev_char = buf.char_clamped(current_char_idx - 1);
        if !prev_char.is_alphanumeric() && prev_char != '_' {
            break;
        }
        current_char_idx -= 1;
    }

    // Check length of word being typed
    if cursor_char_idx <= current_char_idx {
        // Empty word or just whitespace/separator
        if let Some(mut state) = buf.get_state_mut::<CompletionState>().await {
            state.info = None;
            resolver_engine_mut().await.trash_template("lsp_items");
        }
        return;
    }

    // Check if completion is active
    let mut pending_id = None;
    if let Some(state) = buf.get_state::<CompletionState>().await
        && state.info.is_some()
    {
        // Re-request
        pending_id = trigger_completion_request(&mut buf, &mut lsps).await;
    }

    if let Some(id) = pending_id
        && let Some(mut state) = buf.get_state_mut::<CompletionState>().await
        && let Some(info) = &mut state.info
    {
        info.pending_request = id;
    }
}

fn get_match_indices(ranker: &str, text: &str) -> Vec<usize> {
    if ranker.is_empty() || text.is_empty() {
        return vec![];
    }

    let mut indices = vec![];
    let mut i = 0;

    let ranker_chars: Vec<char> = ranker.to_lowercase().chars().collect();
    let text_chars: Vec<char> = text.to_lowercase().chars().collect();

    for (idx, chr) in text_chars.iter().enumerate() {
        if i < ranker_chars.len() && *chr == ranker_chars[i] {
            indices.push(idx);
            i += 1;
        }
    }

    // Only return if full match found (ranking logic implies this)
    if i == ranker_chars.len() {
        indices
    } else {
        vec![]
    }
}

pub async fn render_completions(
    buffers: ResMut<Buffers>,
    grammars: ResMut<GrammarManager>,
    config: Res<ConfigFolder>,
    theme: Res<Theme>,
    log: Res<LogSender>,
) {
    get!(mut buffers, mut grammars, config, theme, log);

    let mut buf = buffers.cur_buffer_mut().await;

    buf.renderer.clear_extmark_ns("lsp::completion");

    let Some(mut state) = buf.get_state_mut::<CompletionState>().await else {
        return;
    };

    let (final_render, cache_update) = {
        let Some(info) = state.info.as_ref() else {
            return;
        };

        // Check if cursor moved before start position (cancelled)
        let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
        let pos = info.position.min(buf.len());

        let start_line = buf.byte_to_line_clamped(pos);
        let current_line = buf.byte_to_line_clamped(cursor_byte);

        if cursor_byte < pos || start_line != current_line {
            state.info = None;
            resolver_engine_mut().await.trash_template("lsp_items");

            return;
        }

        let query = if cursor_byte > pos {
            buf.slice_to_string(pos, cursor_byte).unwrap_or_default()
        } else {
            String::new()
        };

        if info.items.is_empty() {
            // Block ends, returns (None, None)
            (None, None)
        } else {
            // Rank and filter using the shared function
            let ranked_items = get_ranked_items(&info.items, &query);

            if ranked_items.is_empty() {
                (None, None)
            } else {
                // Styles
                let window_style =
                    theme.get_fallback_default(["lsp.autocomplete.window", "ui.window"]);
                let selected_style =
                    theme.get_fallback_default(["lsp.autocomplete.selected", "ui.selection"]);
                let match_style =
                    theme.get_fallback_default(["lsp.autocomplete.match", "ui.match"]);

                // Calculate window
                let window_height = 5;
                let total_items = ranked_items.len();
                let selected_idx = info.selected_index % total_items;

                let half_window = window_height / 2;
                let start_index = if total_items <= window_height || selected_idx <= half_window {
                    0
                } else if selected_idx + half_window >= total_items {
                    total_items - window_height
                } else {
                    selected_idx - half_window
                };

                let items_to_show = ranked_items
                    .iter()
                    .skip(start_index)
                    .take(window_height)
                    .collect::<Vec<_>>();

                let max_label_width = items_to_show
                    .iter()
                    .map(|(i, _)| i.label.len())
                    .max()
                    .unwrap_or(0)
                    .max(10);

                let max_kind_width = items_to_show
                    .iter()
                    .map(|(i, _)| i.kind.map(|k| format!("{:?}", k).len()).unwrap_or(0))
                    .max()
                    .unwrap_or(0);

                let total_width = max_label_width
                    + if max_kind_width > 0 {
                        max_kind_width + 1
                    } else {
                        0
                    };

                let mut list_buf =
                    Buffer::new(vec2(total_width as u16, items_to_show.len() as u16));

                for (i, (item, _)) in items_to_show.iter().enumerate() {
                    let abs_idx = start_index + i;
                    let is_selected = abs_idx == selected_idx;
                    let style = if is_selected {
                        selected_style
                    } else {
                        window_style
                    };

                    let kind_str = item.kind.map(|k| format!("{:?}", k)).unwrap_or_default();
                    let line = if max_kind_width > 0 {
                        format!(
                            "{:<width$} {:>kind_width$}",
                            item.label,
                            kind_str,
                            width = max_label_width,
                            kind_width = max_kind_width
                        )
                    } else {
                        format!("{:<width$}", item.label, width = max_label_width)
                    };

                    for (x, ch) in line.chars().enumerate() {
                        list_buf.set(vec2(x as u16, i as u16), Cell::new(ch.to_string(), style));
                    }

                    let indices = get_match_indices(&query, &item.label);
                    for char_idx in indices {
                        if let Some(cell) = list_buf.get_mut(vec2(char_idx as u16, i as u16)) {
                            if let Some(fg) = match_style.foreground_color {
                                cell.style_mut().foreground_color = Some(fg);
                            } else {
                                cell.style_mut().foreground_color = Some(Color::Blue);
                            }
                        }
                    }
                }

                let mut final_render = Buffer::new(list_buf.size() + vec2(2, 2));
                let mut border = Border::rounded(list_buf.size().x + 2, list_buf.size().y + 2);
                border.style = window_style;
                render!(final_render,
                    (0, 0) => [ border ],
                    (1, 1) => [ list_buf ]
                );

                // Documentation
                let mut doc_rendered = None;
                let mut new_cache_entry = None;

                if let Some((selected_item, _)) = ranked_items.get(selected_idx) {
                    if let Some((idx, buffer)) = &info.cached_doc_buffer
                        && *idx == selected_idx
                    {
                        doc_rendered = Some(buffer.clone());
                    }

                    if doc_rendered.is_none() {
                        let doc = selected_item.documentation.as_ref().map(|d| match d {
                            lsp_types::Documentation::String(s) => s.clone(),
                            lsp_types::Documentation::MarkupContent(m) => m.value.clone(),
                        });

                        let doc_max_width = 40;
                        let mut lines = Vec::new();

                        if let Some(detail) = &selected_item.detail {
                            if !detail.is_empty() {
                                lines.push(vec![(detail.clone(), window_style)]);
                                lines.push(vec![]); // Separator
                            }
                        }

                        if let Some(doc_text) = doc
                            && !doc_text.is_empty()
                        {
                            let highlighted = highlight_text(
                                &doc_text,
                                "markdown",
                                &mut grammars,
                                &config.0,
                                &theme,
                                &log,
                            );

                            let mut current_line = Vec::new();
                            let mut current_width = 0;

                            for (text, style) in highlighted {
                                for char in text.chars() {
                                    if char == '\n' {
                                        lines.push(current_line);
                                        current_line = Vec::new();
                                        current_width = 0;
                                        continue;
                                    }

                                    if current_width >= doc_max_width {
                                        lines.push(current_line);
                                        current_line = Vec::new();
                                        current_width = 0;
                                    }

                                    current_line.push((char.to_string(), style));
                                    current_width += 1;
                                }
                            }
                            if !current_line.is_empty() {
                                lines.push(current_line);
                            }
                        }

                        let doc_height = lines.len().min(15);
                        let doc_width = lines
                            .iter()
                            .take(doc_height)
                            .map(|l| l.iter().map(|x| x.0.width()).sum())
                            .max()
                            .unwrap_or(0);

                        // Always create the doc_buf and doc_box with a border
                        let mut doc_buf = Buffer::new(vec2(doc_width as u16, doc_height as u16));
                        for (y, line) in lines.iter().take(doc_height).enumerate() {
                            for (x, (ch, style)) in line.iter().enumerate() {
                                doc_buf
                                    .set(vec2(x as u16, y as u16), Cell::new(ch.clone(), *style));
                            }
                        }

                        let mut doc_box = Buffer::new(doc_buf.size() + vec2(2, 2));
                        let mut doc_border =
                            Border::rounded(doc_buf.size().x + 2, doc_buf.size().y + 2);
                        doc_border.style = window_style;
                        render!(doc_box,
                           (0, 0) => [ doc_border ],
                           (1, 1) => [ doc_buf ]
                        );

                        let rendered = Arc::new(doc_box);
                        doc_rendered = Some(rendered.clone());
                        new_cache_entry = Some((selected_idx, rendered));
                    }
                }

                if let Some(doc_box) = doc_rendered {
                    let old_size = final_render.size();
                    let new_width = old_size.x + doc_box.size().x;
                    let new_height = old_size.y.max(doc_box.size().y);

                    let mut new_final = Buffer::new(vec2(new_width, new_height));
                    render!(new_final,
                       (0, 0) => [ final_render ],
                       (old_size.x, 0) => [ doc_box.as_ref() ]
                    );
                    final_render = new_final;
                }

                (Some(final_render), new_cache_entry)
            }
        }
    };

    // Handle clearing state if invalid (repeated check, but safe)
    if let Some(info) = &state.info {
        let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
        let pos = info.position.min(buf.len());

        let start_line = buf.byte_to_line_clamped(pos);
        let current_line = buf.byte_to_line_clamped(cursor_byte);

        if cursor_byte < pos || start_line != current_line {
            state.info = None;
            return;
        }
    }

    if let Some((idx, buffer)) = cache_update
        && let Some(info) = &mut state.info
    {
        info.cached_doc_buffer = Some((idx, buffer));
    }

    if let Some(rendered) = final_render {
        let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
        buf.add_extmark(
            ExtmarkBuilder::new("lsp::completion", cursor_byte) // Use current cursor position
                .with_priority(6) // Higher than hover
                .with_decoration(ExtmarkDecoration::OverlayElement {
                    offset: vec2(0, 1), // Render below the line
                    elem: Arc::new(rendered),
                    z_index: 6,
                    clip_to_viewport: true,
                    positioning: OverlayPositioning::RelativeToLine,
                }),
        );
    }
}
