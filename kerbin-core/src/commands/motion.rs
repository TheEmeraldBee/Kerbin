use crate::*;
use regex_cursor::engines::meta::Regex;

#[derive(Debug, Clone, Command)]
pub enum MotionCommand {
    #[command(name = "rx")]
    /// Selects the first match of the regex in the buffer
    Regex {
        pattern: String,
        extend: Option<bool>,
    },

    #[command(name = "rxc")]
    /// Selects the first match of the regex from the cursor
    RegexCursor {
        pattern: String,
        offset: Option<isize>,
        extend: Option<bool>,
    },

    #[command(name = "rxcb")]
    /// Selects the first match of the regex from the cursor
    RegexCursorBackwards {
        pattern: String,
        offset: Option<isize>,
        extend: Option<bool>,
    },

    #[command(name = "rxs")]
    /// Selects the first match of the regex within the selection
    RegexSel { pattern: String },

    #[command(name = "rxsa")]
    /// Creates a cursor at all locations matching the regex in the selection, selecting the
    /// pattern. Does not change selected cursor
    RegexSelAll { pattern: String },

    #[command(drop_ident, name = "sel_line", name = "sl")]
    /// Selects the current (or next if at end of line) line
    /// Extends selection if extend is set
    SelectLine { extend: bool },

    #[command(drop_ident, name = "sel_clear", name = "sc")]
    /// Clears the current cursor selection
    ClearSelection,

    #[command(drop_ident, name = "go_sel_end", name = "gse")]
    /// Sets cursor to the end of the selection
    GotoSelectionEnd,
    #[command(drop_ident, name = "go_sel_begin", name = "gsb")]
    /// Sets cursor to the beginning of the selection
    GotoSelectionBegin,

    #[command(drop_ident, name = "sel_line_end", name = "sle")]
    /// Selects to the end of the line
    /// Extends selection if extend is set
    SelectLineEnd { extend: bool },
    #[command(drop_ident, name = "sel_line_begin", name = "slb")]
    /// Selects from current cursor to the beginning of the line
    /// Extends selection if extend is set
    SelectLineBegin { extend: bool },
    #[command(drop_ident, name = "sel_first_whitespace", name = "sfw")]
    /// Selects to the first non-whitespace character in the line
    /// Extends selection if extend is set
    SelectFirstNonWhitespace { extend: bool },
}

#[async_trait::async_trait]
impl Command for MotionCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut buffers = state.lock_state::<Buffers>().await;
        let mut cur_buffer = buffers.cur_buffer_mut().await;

        let rope_len_bytes = cur_buffer.rope.len();
        let rope_len_lines = cur_buffer.rope.len_lines(LineType::LF_CR);

        match self {
            Self::ClearSelection => {
                let cursor_mut = cur_buffer.primary_cursor_mut();
                let old_sel = cursor_mut.sel().clone();
                cursor_mut.collapse_sel();
                *cursor_mut.sel() != old_sel
            }
            Self::GotoSelectionEnd => {
                let cursor_mut = cur_buffer.primary_cursor_mut();
                cursor_mut.set_at_start(false);
                true
            }
            Self::GotoSelectionBegin => {
                let cursor_mut = cur_buffer.primary_cursor_mut();
                cursor_mut.set_at_start(true);
                true
            }
            MotionCommand::SelectLine { extend } => {
                let current_caret_byte = cur_buffer.primary_cursor().get_cursor_byte();
                let old_sel = cur_buffer.primary_cursor().sel().clone();
                let old_at_start = cur_buffer.primary_cursor().at_start();

                // Get the line containing the cursor
                let line_idx = cur_buffer
                    .rope
                    .byte_to_line_idx(current_caret_byte, LineType::LF_CR);

                // Get start of the line
                let line_start = cur_buffer.rope.line_to_byte_idx(line_idx, LineType::LF_CR);

                // Get end of the line (start of next line - 1, or end of buffer)
                let line_end = if line_idx + 1 >= rope_len_lines {
                    rope_len_bytes
                } else {
                    cur_buffer
                        .rope
                        .line_to_byte_idx(line_idx + 1, LineType::LF_CR)
                        - 1
                };

                if *extend {
                    // When extending, keep the existing selection and extend to include this line
                    let anchor_byte = if old_at_start {
                        *old_sel.end()
                    } else {
                        *old_sel.start()
                    };

                    // Extend selection to include the entire current line
                    let start = anchor_byte.min(line_start);
                    let end = anchor_byte.max(line_end);

                    cur_buffer.primary_cursor_mut().set_sel(start..=end);
                    cur_buffer
                        .primary_cursor_mut()
                        .set_at_start(line_start < anchor_byte);
                } else {
                    // Not extending, select only the current line
                    cur_buffer
                        .primary_cursor_mut()
                        .set_sel(line_start..=line_end);
                    cur_buffer.primary_cursor_mut().set_at_start(false);
                }

                *cur_buffer.primary_cursor().sel() != old_sel
                    || cur_buffer.primary_cursor().at_start() != old_at_start
            }
            MotionCommand::SelectLineEnd { extend } => {
                let current_caret_byte = cur_buffer.primary_cursor().get_cursor_byte();
                let old_sel = cur_buffer.primary_cursor().sel().clone();
                let old_at_start = cur_buffer.primary_cursor().at_start();

                let line_idx = cur_buffer
                    .rope
                    .byte_to_line_idx(current_caret_byte, LineType::LF_CR);
                let line_end_byte = cur_buffer
                    .rope
                    .line_to_byte_idx(line_idx + 1, LineType::LF_CR)
                    .saturating_sub(1);

                let new_caret_byte = line_end_byte;

                let anchor_byte = if *extend == old_at_start {
                    *old_sel.end()
                } else {
                    *old_sel.start()
                };

                let start = anchor_byte.min(new_caret_byte);
                let end = anchor_byte.max(new_caret_byte);
                cur_buffer.primary_cursor_mut().set_sel(start..=end);
                cur_buffer
                    .primary_cursor_mut()
                    .set_at_start(new_caret_byte < anchor_byte);

                *cur_buffer.primary_cursor().sel() != old_sel
                    || cur_buffer.primary_cursor().at_start() != old_at_start
            }
            MotionCommand::SelectLineBegin { extend } => {
                let current_caret_byte = cur_buffer.primary_cursor().get_cursor_byte();
                let old_sel = cur_buffer.primary_cursor().sel().clone();
                let old_at_start = cur_buffer.primary_cursor().at_start();

                let line_idx = cur_buffer
                    .rope
                    .byte_to_line_idx(current_caret_byte, LineType::LF_CR);
                let line_start_byte = cur_buffer.rope.line_to_byte_idx(line_idx, LineType::LF_CR);

                let new_caret_byte = line_start_byte;

                let anchor_byte = if *extend == old_at_start {
                    *old_sel.end()
                } else {
                    *old_sel.start()
                };

                let start = anchor_byte.min(new_caret_byte);
                let end = anchor_byte.max(new_caret_byte);
                cur_buffer.primary_cursor_mut().set_sel(start..=end);
                cur_buffer
                    .primary_cursor_mut()
                    .set_at_start(new_caret_byte < anchor_byte);

                *cur_buffer.primary_cursor().sel() != old_sel
                    || cur_buffer.primary_cursor().at_start() != old_at_start
            }
            MotionCommand::SelectFirstNonWhitespace { extend } => {
                let current_caret_byte = cur_buffer.primary_cursor().get_cursor_byte();
                let old_sel = cur_buffer.primary_cursor().sel().clone();
                let old_at_start = cur_buffer.primary_cursor().at_start();

                let line_idx = cur_buffer
                    .rope
                    .byte_to_line_idx(current_caret_byte, LineType::LF_CR);
                let line_start_byte_idx =
                    cur_buffer.rope.line_to_byte_idx(line_idx, LineType::LF_CR);

                let line_start_char_idx = cur_buffer.rope.byte_to_char_idx(line_start_byte_idx);

                let line_end_char_idx = if line_idx + 1 >= rope_len_lines {
                    cur_buffer.rope.len_chars()
                } else {
                    let char_idx = cur_buffer
                        .rope
                        .line_to_byte_idx(line_idx + 1, LineType::LF_CR)
                        .saturating_sub(1);
                    cur_buffer.rope.byte_to_char_idx(char_idx)
                };

                let mut new_caret_char_idx = line_start_char_idx;
                while new_caret_char_idx < line_end_char_idx
                    && cur_buffer.rope.char(new_caret_char_idx).is_whitespace()
                {
                    new_caret_char_idx += 1;
                }

                let new_caret_byte = cur_buffer.rope.char_to_byte_idx(new_caret_char_idx);

                let anchor_byte = if *extend == old_at_start {
                    *old_sel.end()
                } else {
                    *old_sel.start()
                };

                let start = anchor_byte.min(new_caret_byte);
                let end = anchor_byte.max(new_caret_byte);
                cur_buffer.primary_cursor_mut().set_sel(start..=end);
                cur_buffer
                    .primary_cursor_mut()
                    .set_at_start(new_caret_byte < anchor_byte);

                *cur_buffer.primary_cursor().sel() != old_sel
                    || cur_buffer.primary_cursor().at_start() != old_at_start
            }

            Self::Regex { pattern, extend } => {
                let regex = Regex::new(pattern).unwrap();

                let searcher =
                    regex_cursor::Input::new(RopeyCursor::new(cur_buffer.rope.slice(0..)));
                let x = regex.search(searcher);

                if let Some(x) = x {
                    if extend.unwrap_or(false) {
                        let existing_selection = cur_buffer.primary_cursor().sel().clone();
                        let new_start = (*existing_selection.start()).min(x.start());
                        let new_end = (*existing_selection.end()).max(x.end().saturating_sub(1));
                        cur_buffer.primary_cursor_mut().set_sel(new_start..=new_end);
                    } else {
                        cur_buffer
                            .primary_cursor_mut()
                            .set_sel(x.start()..=x.end().saturating_sub(1));
                    }
                    true
                } else {
                    false
                }
            }

            Self::RegexCursor {
                pattern,
                offset,
                extend,
            } => {
                let regex = Regex::new(pattern).unwrap();

                let cursor = cur_buffer
                    .primary_cursor()
                    .get_cursor_byte()
                    .saturating_add_signed(offset.unwrap_or(0));

                if cursor >= cur_buffer.rope.len() {
                    return false;
                }

                let searcher =
                    regex_cursor::Input::new(RopeyCursor::new(cur_buffer.rope.slice(cursor..)));
                let x = regex.search(searcher);

                if let Some(x) = x {
                    let start = x.start() + cursor;
                    let end = x.end() + cursor;

                    if extend.unwrap_or(false) {
                        let existing_selection = cur_buffer.primary_cursor().sel().clone();
                        let new_start = (*existing_selection.start()).min(start);
                        let new_end = (*existing_selection.end()).max(end.saturating_sub(1));
                        cur_buffer.primary_cursor_mut().set_sel(new_start..=new_end);
                    } else {
                        cur_buffer
                            .primary_cursor_mut()
                            .set_sel(start..=end.saturating_sub(1));
                    }
                    true
                } else {
                    false
                }
            }

            Self::RegexCursorBackwards {
                pattern,
                offset,
                extend,
            } => {
                let regex = Regex::new(pattern).unwrap();

                let cursor = cur_buffer
                    .primary_cursor()
                    .get_cursor_byte()
                    .saturating_add_signed(offset.unwrap_or(0))
                    .min(cur_buffer.rope.len());

                let searcher =
                    regex_cursor::Input::new(RopeyCursor::new(cur_buffer.rope.slice(..cursor)));
                let x = regex.find_iter(searcher);

                if let Some(x) = x.last() {
                    let start = x.start();
                    let end = x.end();

                    if extend.unwrap_or(false) {
                        let existing_selection = cur_buffer.primary_cursor().sel().clone();
                        let new_start = (*existing_selection.start()).min(start);
                        let new_end = (*existing_selection.end()).max(end.saturating_sub(1));
                        cur_buffer.primary_cursor_mut().set_sel(new_start..=new_end);
                    } else {
                        cur_buffer
                            .primary_cursor_mut()
                            .set_sel(start..=end.saturating_sub(1));
                    }
                    true
                } else {
                    false
                }
            }

            Self::RegexSel { pattern } => {
                let regex = Regex::new(pattern).unwrap();

                let cursor = cur_buffer.primary_cursor().sel();
                let searcher = regex_cursor::Input::new(RopeyCursor::new(
                    cur_buffer.rope.slice(cursor.clone()),
                ));
                let x = regex.search(searcher);

                if let Some(x) = x {
                    let start = x.start() + *cursor.start();
                    let end = x.end() + *cursor.start();

                    cur_buffer
                        .primary_cursor_mut()
                        .set_sel(start..=end.saturating_sub(1));

                    true
                } else {
                    false
                }
            }
            Self::RegexSelAll { pattern } => {
                let regex = Regex::new(pattern).unwrap();

                let cursor = cur_buffer.primary_cursor().sel();
                let searcher = regex_cursor::Input::new(RopeyCursor::new(
                    cur_buffer.rope.slice(cursor.clone()),
                ));

                let initial_cursor = cur_buffer.primary_cursor;

                let mut ranges = vec![];

                for match_ in regex.find_iter(searcher) {
                    ranges.push(match_.start()..=match_.end().saturating_sub(1));
                }

                for range in ranges {
                    cur_buffer.create_cursor();
                    cur_buffer.primary_cursor_mut().set_sel(range);
                }

                if cur_buffer.primary_cursor != initial_cursor {
                    cur_buffer.cursors.remove(initial_cursor);
                    cur_buffer.primary_cursor -= 1;
                    true
                } else {
                    false
                }
            }
        }
    }
}
