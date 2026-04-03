use std::sync::Arc;

use kerbin_core::*;
use lsp_types::{
    CompletionItem, CompletionParams, CompletionResponse, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};
use ratatui::{
    layout::Rect,
    prelude::*,
    widgets::{Block, BorderType, Paragraph},
};

use ropey::RopeSlice;

use crate::{text_edit::apply_text_edits_inner, JsonRpcMessage, LspManager, OpenedFile};
use kerbin_tree_sitter::{grammar_manager::GrammarManager, state::highlight_text};

struct CompletionWidget(ratatui::buffer::Buffer);

impl OverlayWidget for CompletionWidget {
    fn dimensions(&self) -> (u16, u16) {
        (self.0.area.width, self.0.area.height)
    }

    fn render(&self, _area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let src_area = self.0.area;
        for cy in 0..src_area.height {
            for cx in 0..src_area.width {
                if let (Some(src), Some(dst)) = (
                    self.0.cell((src_area.x + cx, src_area.y + cy)),
                    buf.cell_mut((cx, cy)),
                ) {
                    *dst = src.clone();
                }
            }
        }
    }
}

pub struct CompletionInfo {
    pub pending_request: i32,
    pub pending_resolve: Option<(i32, usize)>, // (resolve_request_id, raw index into items)
    pub items: Vec<CompletionItem>,
    pub position: usize,
    pub selected_index: usize,
    pub cached_doc_buffer: Option<(usize, Arc<ratatui::buffer::Buffer>)>,
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
    let line_start = buf.line_to_byte_clamped(line);
    let character: usize = buf
        .slice(line_start, cursor_byte)
        .map(|s| s.chars().map(|c| c.len_utf16()).sum())
        .unwrap_or(0);

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

async fn trigger_resolve_request(
    buf: &TextBuffer,
    lsps: &mut LspManager,
    item: &CompletionItem,
) -> Option<i32> {
    let file = buf.get_state::<OpenedFile>().await?;
    let client = lsps.get_or_create_client(&file.lang).await?;
    client.request("completionItem/resolve", item.clone()).await.ok()
}

/// Sends a `completionItem/resolve` request for the currently selected ranked item and
/// stores the request ID + raw item index in `info.pending_resolve`.
async fn send_resolve_for_selected(buf: &TextBuffer, lsps: &mut LspManager, info: &mut CompletionInfo) {
    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
    let pos = info.position.min(buf.len());
    let query = if cursor_byte >= pos {
        buf.slice_to_string(pos, cursor_byte).unwrap_or_default()
    } else {
        String::new()
    };

    // Compute clone + raw index in a block so *const pointers don't cross the await below
    let resolve_target = {
        let ranked = get_ranked_items(&info.items, &query);
        if ranked.is_empty() {
            None
        } else {
            let ranked_idx = info.selected_index % ranked.len();
            let selected_ptr = ranked[ranked_idx].0 as *const CompletionItem;
            info.items
                .iter()
                .position(|x| std::ptr::eq(x, selected_ptr))
                .map(|raw_idx| (info.items[raw_idx].clone(), raw_idx))
        }
    };

    let Some((item_clone, raw_idx)) = resolve_target else { return; };
    if let Some(id) = trigger_resolve_request(buf, lsps, &item_clone).await {
        info.pending_resolve = Some((id, raw_idx));
    }
}

fn get_ranked_items<'a>(
    items: &'a [CompletionItem],
    query: &str,
) -> Vec<(&'a CompletionItem, i32)> {
    if query.is_empty() {
        let mut all: Vec<_> = items.iter().map(|item| (item, 0)).collect();
        all.sort_by(|(a, _), (b, _)| match (&a.sort_text, &b.sort_text) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        return all;
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
        quality_b
            .cmp(quality_a)
            .then_with(|| match (&item_a.sort_text, &item_b.sort_text) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
    });

    matched_items
        .into_iter()
        .map(|(item, _, _)| (item, 0))
        .collect()
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

    if i == ranker_chars.len() {
        indices
    } else {
        vec![]
    }
}

struct ListPopupStyles {
    window: Style,
    selected: Style,
    match_hl: Style,
}

fn build_list_popup(
    items_to_show: &[(&CompletionItem, i32)],
    start_index: usize,
    selected_idx: usize,
    query: &str,
    max_label_width: usize,
    max_kind_width: usize,
    styles: ListPopupStyles,
) -> ratatui::buffer::Buffer {
    let window_style = styles.window;
    let selected_style = styles.selected;
    let match_style = styles.match_hl;
    let inner_w = (max_label_width
        + if max_kind_width > 0 {
            max_kind_width + 1
        } else {
            0
        }) as u16;
    let inner_h = items_to_show.len() as u16;
    let popup_rect = Rect::new(0, 0, inner_w + 2, inner_h + 2);
    let mut buf = ratatui::buffer::Buffer::empty(popup_rect);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(window_style);
    let inner = block.inner(popup_rect);
    block.render(popup_rect, &mut buf);

    let lines: Vec<Line<'static>> = items_to_show
        .iter()
        .enumerate()
        .map(|(i, (item, _))| {
            let abs_idx = start_index + i;
            let is_selected = abs_idx == selected_idx;
            let row_style = if is_selected {
                selected_style
            } else {
                window_style
            };

            let kind_str = item.kind.map(|k| format!("{:?}", k)).unwrap_or_default();
            let line_str = if max_kind_width > 0 {
                format!(
                    "{:<lw$} {:>kw$}",
                    item.label,
                    kind_str,
                    lw = max_label_width,
                    kw = max_kind_width
                )
            } else {
                format!("{:<lw$}", item.label, lw = max_label_width)
            };

            let match_indices = get_match_indices(query, &item.label);
            Line::from(
                line_str
                    .chars()
                    .enumerate()
                    .map(|(x, ch)| {
                        let char_style = if x < item.label.len() && match_indices.contains(&x) {
                            match_style
                        } else {
                            row_style
                        };
                        Span::styled(ch.to_string(), char_style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    Paragraph::new(Text::from(lines)).render(inner, &mut buf);

    buf
}

fn build_doc_popup(
    lines: &[Vec<(String, Style)>],
    doc_height: usize,
    doc_width: usize,
    window_style: Style,
) -> ratatui::buffer::Buffer {
    let popup_rect = Rect::new(0, 0, doc_width as u16 + 2, doc_height as u16 + 2);
    let mut buf = ratatui::buffer::Buffer::empty(popup_rect);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(window_style);
    let inner = block.inner(popup_rect);
    block.render(popup_rect, &mut buf);

    let text_lines: Vec<Line<'static>> = lines
        .iter()
        .take(doc_height)
        .map(|line| {
            Line::from(
                line.iter()
                    .flat_map(|(text, style)| {
                        text.chars()
                            .map(|ch| Span::styled(ch.to_string(), *style))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    Paragraph::new(Text::from(text_lines)).render(inner, &mut buf);

    buf
}

fn combine_side_by_side(
    left: ratatui::buffer::Buffer,
    right: ratatui::buffer::Buffer,
) -> ratatui::buffer::Buffer {
    let combined_w = left.area.width + right.area.width;
    let combined_h = left.area.height.max(right.area.height);
    let combined_rect = Rect::new(0, 0, combined_w, combined_h);
    let mut combined = ratatui::buffer::Buffer::empty(combined_rect);

    for y in 0..left.area.height {
        for x in 0..left.area.width {
            if let (Some(src), Some(dst)) = (left.cell((x, y)), combined.cell_mut((x, y))) {
                *dst = src.clone();
            }
        }
    }
    let offset = left.area.width;
    for y in 0..right.area.height {
        for x in 0..right.area.width {
            if let (Some(src), Some(dst)) = (right.cell((x, y)), combined.cell_mut((offset + x, y)))
            {
                *dst = src.clone();
            }
        }
    }

    combined
}

#[async_trait::async_trait]
impl Command<State> for CompletionCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::StartRequest => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut lsps = state.lock_state::<LspManager>().await;

                let Some(mut buf) = bufs.cur_text_buffer_mut().await else { return true; };

                let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());

                let mut state = buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(existing_info) = &state.info {
                    let pos = existing_info.position.min(buf.len());
                    let start_line = buf.byte_to_line_clamped(pos);
                    let current_line = buf.byte_to_line_clamped(cursor_byte);

                    if cursor_byte < pos || start_line != current_line {
                        state.info = None;
                    } else {
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

                    if cursor_char_idx <= current_char_idx {
                        return true;
                    }

                    if current_char_idx < cursor_char_idx {
                        start_pos = buf.char_to_byte_clamped(current_char_idx);
                    }

                    state.info = Some(CompletionInfo {
                        pending_request: id,
                        pending_resolve: None,
                        items: vec![],
                        position: start_pos,
                        selected_index: 0,
                        cached_doc_buffer: None,
                    });
                }
            }
            Self::Accept => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let Some(mut buf) = bufs.cur_text_buffer_mut().await else { return true; };

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

                        let ranked_items = get_ranked_items(&info.items, &query);

                        if !ranked_items.is_empty() {
                            let idx = info.selected_index % ranked_items.len();
                            if let Some((item, _)) = ranked_items.get(idx) {
                                let (start_byte, end_byte, text) =
                                    if let Some(lsp_types::CompletionTextEdit::Edit(e)) =
                                        &item.text_edit
                                    {
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
                                            + start_line_slice.char_to_byte(start_char);

                                        let end_line_slice = buf.line_clamped(end_line);
                                        let end_char = (e.range.end.character as usize)
                                            .min(line_content_len(&end_line_slice));
                                        let end = buf.line_to_byte_clamped(end_line)
                                            + end_line_slice.char_to_byte(end_char);

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

                                buf.start_change_group();

                                if end_byte > start_byte {
                                    let len_chars = buf.byte_to_char_clamped(end_byte)
                                        - buf.byte_to_char_clamped(start_byte);
                                    buf.action(kerbin_core::buffer::action::Delete {
                                        byte: start_byte,
                                        len: len_chars,
                                    });
                                }

                                buf.action(kerbin_core::buffer::action::Insert {
                                    byte: start_byte,
                                    content: text.clone(),
                                });

                                buf.primary_cursor_mut()
                                    .set_sel(start_byte + text.len()..=start_byte + text.len());

                                if let Some(additional_edits) = item.additional_text_edits.clone() {
                                    apply_text_edits_inner(&mut buf, additional_edits);
                                }

                                buf.commit_change_group();
                            }
                        }
                    }
                }

                completion_state.info = None;
                completion_state.just_accepted = true;
                resolver_engine_mut().await.remove_template("lsp_items");
            }
            Self::Trash => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let Some(mut buf) = bufs.cur_text_buffer_mut().await else { return true; };

                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                completion_state.info = None;
                resolver_engine_mut().await.remove_template("lsp_items");
            }
            Self::SelectNext => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut lsps = state.lock_state::<LspManager>().await;
                let Some(mut buf) = bufs.cur_text_buffer_mut().await else { return true; };
                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(info) = &mut completion_state.info {
                    info.selected_index += 1;
                    info.cached_doc_buffer = None;
                    send_resolve_for_selected(&buf, &mut lsps, info).await;
                }
            }
            Self::SelectPrevious => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut lsps = state.lock_state::<LspManager>().await;
                let Some(mut buf) = bufs.cur_text_buffer_mut().await else { return true; };
                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(info) = &mut completion_state.info
                    && info.selected_index > 0
                {
                    info.selected_index -= 1;
                    info.cached_doc_buffer = None;
                    send_resolve_for_selected(&buf, &mut lsps, info).await;
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
            let buf_guard = buf.read().await;
            if let Some(text_buf) = buf_guard.downcast::<TextBuffer>()
                && let Some(state) = text_buf.get_state::<CompletionState>().await
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

        let mut buf_guard = buf.write_owned().await;
        let Some(buf) = buf_guard.downcast_mut::<TextBuffer>() else { return; };
        let Some(mut completion_state) = buf.get_state_mut::<CompletionState>().await else { return; };
        let Some(info) = completion_state.info.as_mut() else { return; };

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
        info.cached_doc_buffer = None;

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

        resolver_engine_mut().await.set_template("lsp_items", Token::list_from(items));

        // Trigger resolve for item 0 so additionalTextEdits are ready before the user accepts.
        // Safe to lock LspManager here: ProcessLspEventsCommand releases it before calling handlers.
        let first_item = info.items.first().cloned();
        if let Some(first_item) = first_item {
            let mut lsps = state.lock_state::<LspManager>().await;
            if let Some(id) = trigger_resolve_request(buf, &mut lsps, &first_item).await {
                info.pending_resolve = Some((id, 0));
            }
        }
    }
}

pub async fn handle_completion_resolve(state: &State, msg: &JsonRpcMessage) {
    let JsonRpcMessage::Response(response) = msg else { return; };

    let bufs = state.lock_state::<Buffers>().await;
    let mut buffer = None;
    let mut raw_idx = 0usize;
    for buf in &bufs.buffers {
        let buf_guard = buf.read().await;
        if let Some(text_buf) = buf_guard.downcast::<TextBuffer>()
            && let Some(cs) = text_buf.get_state::<CompletionState>().await
            && let Some(info) = &cs.info
            && let Some((resolve_id, idx)) = info.pending_resolve
            && resolve_id == response.id
        {
            buffer = Some(buf.clone());
            raw_idx = idx;
            break;
        }
    }
    drop(bufs);

    let Some(buf_arc) = buffer else { return; };
    let mut buf_guard = buf_arc.write_owned().await;
    let Some(buf) = buf_guard.downcast_mut::<TextBuffer>() else { return; };
    let Some(mut completion_state) = buf.get_state_mut::<CompletionState>().await else { return; };
    let Some(info) = completion_state.info.as_mut() else { return; };

    // Guard against stale response (user navigated to a different item)
    if info.pending_resolve.map(|(id, _)| id) != Some(response.id) {
        return;
    }
    info.pending_resolve = None;

    let Some(result) = &response.result else { return; };
    let Ok(resolved) = serde_json::from_value::<CompletionItem>(result.clone()) else { return; };

    if let Some(existing) = info.items.get_mut(raw_idx) {
        if resolved.additional_text_edits.is_some() {
            existing.additional_text_edits = resolved.additional_text_edits;
        }
        if resolved.documentation.is_some() && existing.documentation.is_none() {
            existing.documentation = resolved.documentation;
        }
        if resolved.detail.is_some() && existing.detail.is_none() {
            existing.detail = resolved.detail;
        }
        info.cached_doc_buffer = None;
    }
}

pub async fn update_completions(bufs: ResMut<Buffers>, lsps: ResMut<LspManager>) {
    get!(mut bufs, mut lsps);

    let Some(mut buf) = bufs.cur_text_buffer_mut().await else { return; };

    if buf.byte_changes.is_empty() {
        return;
    }

    if let Some(mut state) = buf.get_state_mut::<CompletionState>().await
        && state.just_accepted
    {
        state.just_accepted = false;
        state.info = None;
        resolver_engine_mut().await.remove_template("lsp_items");
        return;
    }

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

    if cursor_char_idx <= current_char_idx {
        if let Some(mut state) = buf.get_state_mut::<CompletionState>().await {
            state.info = None;
            resolver_engine_mut().await.remove_template("lsp_items");
        }
        return;
    }

    let mut pending_id = None;
    if let Some(state) = buf.get_state::<CompletionState>().await
        && state.info.is_some()
    {
        pending_id = trigger_completion_request(&mut buf, &mut lsps).await;
    }

    if let Some(id) = pending_id
        && let Some(mut state) = buf.get_state_mut::<CompletionState>().await
        && let Some(info) = &mut state.info
    {
        info.pending_request = id;
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

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else { return; };

    buf.renderer.clear_extmark_ns("lsp::completion");

    let Some(mut state) = buf.get_state_mut::<CompletionState>().await else {
        return;
    };

    let Some(info) = state.info.as_ref() else {
        return;
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
    let pos = info.position.min(buf.len());

    let start_line = buf.byte_to_line_clamped(pos);
    let current_line = buf.byte_to_line_clamped(cursor_byte);

    if cursor_byte < pos || start_line != current_line {
        state.info = None;
        resolver_engine_mut().await.remove_template("lsp_items");
        return;
    }

    let query = if cursor_byte > pos {
        buf.slice_to_string(pos, cursor_byte).unwrap_or_default()
    } else {
        String::new()
    };

    let ranked_items = get_ranked_items(&info.items, &query);

    if ranked_items.is_empty() {
        return;
    }

    let window_style = theme.get_fallback_default(["lsp.autocomplete.window", "ui.window"]);
    let selected_style = theme.get_fallback_default(["lsp.autocomplete.selected", "ui.selection"]);
    let match_style = theme.get_fallback_default(["lsp.autocomplete.match", "ui.match"]);

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

    let items_to_show: Vec<_> = ranked_items
        .iter()
        .skip(start_index)
        .take(window_height)
        .collect();

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

    let list_popup = build_list_popup(
        &items_to_show
            .iter()
            .map(|&&(item, score)| (item, score))
            .collect::<Vec<_>>(),
        start_index,
        selected_idx,
        &query,
        max_label_width,
        max_kind_width,
        ListPopupStyles {
            window: window_style,
            selected: selected_style,
            match_hl: match_style,
        },
    );

    let (final_popup, cache_update) = {
        let mut doc_rendered: Option<Arc<ratatui::buffer::Buffer>> = None;
        let mut new_cache = None;

        if let Some((selected_item, _)) = ranked_items.get(selected_idx) {
            if let Some((cached_idx, ref cached_buf)) = info.cached_doc_buffer
                && cached_idx == selected_idx
            {
                doc_rendered = Some(cached_buf.clone());
            }

            if doc_rendered.is_none() {
                let doc_text = selected_item.documentation.as_ref().map(|d| match d {
                    lsp_types::Documentation::String(s) => s.clone(),
                    lsp_types::Documentation::MarkupContent(m) => m.value.clone(),
                });

                let doc_max_width = 40usize;
                let mut lines: Vec<Vec<(String, Style)>> = Vec::new();

                if let Some(detail) = &selected_item.detail
                    && !detail.is_empty()
                {
                    lines.push(vec![(detail.clone(), window_style)]);
                    lines.push(vec![]);
                }

                if let Some(text) = doc_text
                    && !text.is_empty()
                {
                    let highlighted =
                        highlight_text(&text, "markdown", &mut grammars, &config.0, &theme, &log);

                    let mut current_line: Vec<(String, Style)> = Vec::new();
                    let mut current_width = 0;

                    for (part, style) in highlighted {
                        for ch in part.chars() {
                            if ch == '\n' {
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
                            current_line.push((ch.to_string(), style));
                            current_width += 1;
                        }
                    }
                    if !current_line.is_empty() {
                        lines.push(current_line);
                    }
                }

                if !lines.is_empty() {
                    let doc_height = lines.len().min(15);
                    let doc_width = lines
                        .iter()
                        .take(doc_height)
                        .map(|l| l.iter().map(|(s, _)| s.chars().count()).sum::<usize>())
                        .max()
                        .unwrap_or(0)
                        .max(1);

                    let doc_buf = build_doc_popup(&lines, doc_height, doc_width, window_style);
                    let doc_arc = Arc::new(doc_buf);
                    doc_rendered = Some(doc_arc.clone());
                    new_cache = Some((selected_idx, doc_arc));
                }
            }
        }

        let final_popup = if let Some(doc) = doc_rendered {
            combine_side_by_side(
                list_popup,
                Arc::try_unwrap(doc).unwrap_or_else(|arc| (*arc).clone()),
            )
        } else {
            list_popup
        };

        (final_popup, new_cache)
    };

    let position = pos;

    if let Some((idx, buf_arc)) = cache_update
        && let Some(info) = &mut state.info
    {
        info.cached_doc_buffer = Some((idx, buf_arc));
    }

    buf.add_extmark(
        ExtmarkBuilder::new("lsp::completion", position)
            .with_priority(6)
            .with_kind(ExtmarkKind::Overlay {
                widget: Arc::new(CompletionWidget(final_popup)),
                position: OverlayPosition::Smart,
                z_index: 6,
            }),
    );
}
