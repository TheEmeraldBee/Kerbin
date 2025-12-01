use std::sync::Arc;

use kerbin_core::{
    TextBuffer,
    ascii_forge::{prelude::*, widgets::Border},
    *,
};
use lsp_types::{
    CompletionItem, CompletionParams, CompletionResponse, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};
use ropey::RopeSlice;

use crate::{JsonRpcMessage, LspManager, OpenedFile};

pub struct CompletionInfo {
    pub pending_request: i32,
    pub items: Vec<CompletionItem>,
    pub position: usize,
}

#[derive(State, Default)]
pub struct CompletionState {
    pub info: Option<CompletionInfo>,
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
}

async fn trigger_completion_request(buf: &mut TextBuffer, lsps: &mut LspManager) -> Option<i32> {
    let file = buf.get_state::<OpenedFile>().await?;

    let client = lsps.get_or_create_client(&file.lang).await?;

    let cursor = buf.primary_cursor();
    let cursor_byte = cursor.get_cursor_byte().min(buf.len_bytes());

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
    let mut ranked_items: Vec<(&CompletionItem, i32)> = items
        .iter()
        .filter_map(|item| {
            kerbin_core::palette::ranking::rank(query, &item.label).map(|score| (item, score))
        })
        .collect();

    ranked_items.sort_by_key(|(_, score)| *score);
    ranked_items
}

#[async_trait::async_trait]
impl Command for CompletionCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::StartRequest => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut lsps = state.lock_state::<LspManager>().await;

                let mut buf = bufs.cur_buffer_mut().await;

                let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len_bytes());

                // Check if there's already an active completion
                let mut state = buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(existing_info) = &state.info {
                    let pos = existing_info.position.min(buf.len_bytes());
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

                    if current_char_idx < cursor_char_idx {
                        start_pos = buf.char_to_byte_clamped(current_char_idx);
                    }

                    state.info = Some(CompletionInfo {
                        pending_request: id,
                        items: vec![],
                        position: start_pos,
                    });
                }
            }
            Self::Accept => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut buf = bufs.cur_buffer_mut().await;

                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                if let Some(info) = &completion_state.info {
                    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len_bytes());
                    let pos = info.position.min(buf.len_bytes());

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

                        // Get the first item from ranked items (what's visually first)
                        if let Some((item, _)) = ranked_items.first() {
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
                                                    if len > 0 && line_slice.char(len - 1) == '\r' {
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
                                    let start_line = (e.range.start.line as usize).min(max_line);
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
                                        item.insert_text.as_ref().unwrap_or(&item.label).clone(),
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

                completion_state.info = None;
            }
            Self::Trash => {
                let mut bufs = state.lock_state::<Buffers>().await;
                let mut buf = bufs.cur_buffer_mut().await;

                let mut completion_state =
                    buf.get_or_insert_state_mut(CompletionState::default).await;

                // Clear completion state
                completion_state.info = None;
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

        let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len_bytes());
        let pos = info.position.min(buf.len_bytes());

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

pub async fn render_completions(buffers: ResMut<Buffers>) {
    get!(mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    buf.renderer.clear_extmark_ns("lsp::completion");

    let Some(mut state) = buf.get_state_mut::<CompletionState>().await else {
        return;
    };

    let Some(info) = state.info.as_ref() else {
        return;
    };

    // Check if cursor moved before start position (cancelled)
    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len_bytes());
    let pos = info.position.min(buf.len_bytes());

    let start_line = buf.byte_to_line_clamped(pos);
    let current_line = buf.byte_to_line_clamped(cursor_byte);

    if cursor_byte < pos || start_line != current_line {
        state.info = None;
        return;
    }

    let query = if cursor_byte > pos {
        buf.slice_to_string(pos, cursor_byte).unwrap_or_default()
    } else {
        String::new()
    };

    if info.items.is_empty() {
        return;
    }

    // Rank and filter using the shared function
    let ranked_items = get_ranked_items(&info.items, &query);

    let items_to_show = ranked_items
        .iter()
        .take(5)
        .map(|(item, _)| item)
        .collect::<Vec<_>>();

    if items_to_show.is_empty() {
        state.info = None;
        return;
    }

    let max_width = items_to_show
        .iter()
        .map(|i| i.label.len())
        .max()
        .unwrap_or(0)
        .max(10);

    let mut text_lines = Vec::new();
    for item in &items_to_show {
        text_lines.push(format!("{:<width$}", item.label, width = max_width));
    }

    let text_content = text_lines.join("\n");
    let mut text = Buffer::sized_element(text_content);

    // Highlight matches
    for (line_idx, item) in items_to_show.iter().enumerate() {
        let indices = get_match_indices(&query, &item.label);
        for char_idx in indices {
            // Using try_into().unwrap_or(0) to safely cast to u16.
            // In a real scenario, should probably handle bounds better or ensure max_width fits.
            if let Some(cell) = text.get_mut(vec2(
                (char_idx as i32).try_into().unwrap_or(0),
                (line_idx as i32).try_into().unwrap_or(0),
            )) {
                cell.style_mut().foreground_color = Some(Color::Blue);
            }
        }
    }

    let mut render = Buffer::new(text.size() + vec2(2, 2));

    render!(render,
        (0, 0) => [ Border::rounded(text.size().x + 2, text.size().y + 2) ],
        (1, 1) => [ text ]
    );

    buf.add_extmark(
        ExtmarkBuilder::new("lsp::completion", cursor_byte) // Use current cursor position
            .with_priority(6) // Higher than hover
            .with_decoration(ExtmarkDecoration::OverlayElement {
                offset: vec2(0, 1), // Render below the line
                elem: Arc::new(render),
                z_index: 6,
                clip_to_viewport: true,
                positioning: OverlayPositioning::RelativeToLine,
            }),
    );
}
