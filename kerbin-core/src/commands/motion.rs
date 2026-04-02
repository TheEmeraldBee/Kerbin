use crate::*;
use regex_cursor::engines::meta::Regex;

/// Applies a regex match range to the primary cursor, handling extend vs replace.
/// `end` is the exclusive end from the regex engine; the cursor selection is inclusive.
fn apply_match_sel(buf: &mut TextBuffer, start: usize, end: usize, extend: bool) {
    let sel_end = end.saturating_sub(1);
    if extend {
        let existing = buf.primary_cursor().sel().clone();
        let new_start = (*existing.start()).min(start);
        let new_end = (*existing.end()).max(sel_end);
        buf.primary_cursor_mut().set_sel(new_start..=new_end);
    } else {
        buf.primary_cursor_mut().set_sel(start..=sel_end);
    }
}

fn rope_input(slice: ropey::RopeSlice<'_>) -> regex_cursor::Input<RopeyCursor<'_>> {
    regex_cursor::Input::new(RopeyCursor::new(slice))
}

fn try_compile_regex(pattern: &str, log: &LogSender) -> Option<Regex> {
    match Regex::new(pattern) {
        Ok(r) => Some(r),
        Err(e) => {
            log.high("command::motion", format!("Invalid regex: {e}"));
            None
        }
    }
}

#[derive(Debug, Clone, Command)]
pub enum MotionCommand {
    #[command(name = "rx")]
    /// Selects the first match of the regex in the buffer
    Regex {
        pattern: String,
        #[command(flag)]
        extend: bool,
    },

    #[command(name = "rxc")]
    /// Selects the first match of the regex from the cursor
    /// Use --offset to start searching from an offset relative to the cursor
    /// Use --advance to guarantee forward progress
    RegexCursor {
        pattern: String,
        #[command(flag)]
        offset: Option<isize>,
        #[command(flag)]
        extend: bool,
        #[command(flag)]
        advance: bool,
    },

    #[command(name = "rxcb")]
    /// Selects the last match of the regex before the cursor
    /// Use --offset to start searching from an offset relative to the cursor
    /// Use --advance to guarantee backward progress
    RegexCursorBackwards {
        pattern: String,
        #[command(flag)]
        offset: Option<isize>,
        #[command(flag)]
        extend: bool,
        #[command(flag)]
        advance: bool,
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
    /// Use --extend to extend the existing selection
    SelectLine {
        #[command(flag)]
        extend: bool,
    },

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
    /// Use --extend to extend the existing selection
    SelectLineEnd {
        #[command(flag)]
        extend: bool,
    },
    #[command(drop_ident, name = "sel_line_begin", name = "slb")]
    /// Selects from current cursor to the beginning of the line
    /// Use --extend to extend the existing selection
    SelectLineBegin {
        #[command(flag)]
        extend: bool,
    },
    #[command(drop_ident, name = "sel_first_whitespace", name = "sfw")]
    /// Selects to the first non-whitespace character in the line
    /// Use --extend to extend the existing selection
    SelectFirstNonWhitespace {
        #[command(flag)]
        extend: bool,
    },
}

#[async_trait::async_trait]
impl Command<State> for MotionCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let log = state.lock_state::<LogSender>().await;
        let mut buffers = state.lock_state::<Buffers>().await;
        let Some(mut cur_buffer) = buffers.cur_text_buffer_mut().await else {
            return false;
        };

        let rope_len_bytes = cur_buffer.len();
        let rope_len_lines = cur_buffer.len_lines();

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

                let line_idx = cur_buffer.byte_to_line_clamped(current_caret_byte);
                let line_start = cur_buffer.line_to_byte_clamped(line_idx);
                let line_end = if line_idx + 1 >= rope_len_lines {
                    rope_len_bytes
                } else {
                    cur_buffer.line_to_byte_clamped(line_idx + 1) - 1
                };

                if *extend {
                    let anchor_byte = if old_at_start {
                        *old_sel.end()
                    } else {
                        *old_sel.start()
                    };

                    let start = anchor_byte.min(line_start);
                    let end = anchor_byte.max(line_end);

                    cur_buffer.primary_cursor_mut().set_sel(start..=end);
                    cur_buffer
                        .primary_cursor_mut()
                        .set_at_start(line_start < anchor_byte);
                } else {
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

                let line_idx = cur_buffer.byte_to_line_clamped(current_caret_byte);
                let line_end_byte = cur_buffer
                    .line_to_byte(line_idx + 1)
                    .map(|b| b.saturating_sub(1))
                    .unwrap_or_else(|| cur_buffer.len());

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

                let line_idx = cur_buffer.byte_to_line_clamped(current_caret_byte);
                let line_start_byte = cur_buffer.line_to_byte_clamped(line_idx);

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

                let line_idx = cur_buffer.byte_to_line_clamped(current_caret_byte);
                let line_start_byte_idx = cur_buffer.line_to_byte_clamped(line_idx);

                let line_start_char_idx = cur_buffer.byte_to_char_clamped(line_start_byte_idx);

                let line_end_char_idx = if line_idx + 1 >= rope_len_lines {
                    cur_buffer.len_chars()
                } else {
                    let char_idx = cur_buffer
                        .line_to_byte_clamped(line_idx + 1)
                        .saturating_sub(1);
                    cur_buffer.byte_to_char_clamped(char_idx)
                };

                let mut new_caret_char_idx = line_start_char_idx;
                while new_caret_char_idx < line_end_char_idx
                    && cur_buffer.char_clamped(new_caret_char_idx).is_whitespace()
                {
                    new_caret_char_idx += 1;
                }

                let new_caret_byte = cur_buffer.char_to_byte_clamped(new_caret_char_idx);

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
                let Some(regex) = try_compile_regex(pattern, &log) else { return false; };

                let len = cur_buffer.len();
                if let Some(m) = regex.search(rope_input(cur_buffer.slice_clamped(0, len))) {
                    apply_match_sel(&mut cur_buffer, m.start(), m.end(), *extend);
                    true
                } else {
                    false
                }
            }

            Self::RegexCursor {
                pattern,
                offset,
                extend,
                advance,
            } => {
                let Some(regex) = try_compile_regex(pattern, &log) else { return false; };

                let base_cursor = cur_buffer
                    .primary_cursor()
                    .get_cursor_byte()
                    .saturating_add_signed(offset.unwrap_or(0));

                let len = cur_buffer.len();
                if base_cursor >= len {
                    return false;
                }

                if *advance {
                    let original_end = *cur_buffer.primary_cursor().sel().end();
                    let mut search_from = base_cursor;

                    loop {
                        if search_from >= len { return false; }

                        let Some(x) = regex.search(rope_input(cur_buffer.slice_clamped(search_from, len))) else {
                            return false;
                        };

                        let start = x.start() + search_from;
                        let end = x.end() + search_from;

                        if end.saturating_sub(1) != original_end {
                            apply_match_sel(&mut cur_buffer, start, end, *extend);
                            return true;
                        }

                        search_from += 1;
                    }
                }

                if let Some(m) = regex.search(rope_input(cur_buffer.slice_clamped(base_cursor, len))) {
                    let start = m.start() + base_cursor;
                    let end = m.end() + base_cursor;
                    apply_match_sel(&mut cur_buffer, start, end, *extend);
                    true
                } else {
                    false
                }
            }

            Self::RegexCursorBackwards {
                pattern,
                offset,
                extend,
                advance,
            } => {
                let Some(regex) = try_compile_regex(pattern, &log) else { return false; };

                let base_cursor = cur_buffer
                    .primary_cursor()
                    .get_cursor_byte()
                    .saturating_add_signed(offset.unwrap_or(0))
                    .min(cur_buffer.len());

                if *advance {
                    let original_start = *cur_buffer.primary_cursor().sel().start();
                    let mut search_ceil = base_cursor;

                    loop {
                        if search_ceil == 0 { return false; }

                        let Some(x) = regex.find_iter(rope_input(cur_buffer.slice_clamped(0, search_ceil))).last() else {
                            return false;
                        };

                        let start = x.start();
                        let end = x.end();

                        if start != original_start {
                            apply_match_sel(&mut cur_buffer, start, end, *extend);
                            return true;
                        }

                        search_ceil -= 1;
                    }
                }

                if let Some(m) = regex.find_iter(rope_input(cur_buffer.slice_clamped(0, base_cursor))).last() {
                    apply_match_sel(&mut cur_buffer, m.start(), m.end(), *extend);
                    true
                } else {
                    false
                }
            }

            Self::RegexSel { pattern } => {
                let Some(regex) = try_compile_regex(pattern, &log) else { return false; };

                let start_idx = *cur_buffer.primary_cursor().sel().start();
                let end_idx = *cur_buffer.primary_cursor().sel().end() + 1;

                let slice = cur_buffer.slice_clamped(start_idx, end_idx);
                if let Some(x) = regex.search(rope_input(slice)) {
                    let start = x.start() + start_idx;
                    let end = x.end() + start_idx;
                    apply_match_sel(&mut cur_buffer, start, end, false);
                    true
                } else {
                    false
                }
            }

            Self::RegexSelAll { pattern } => {
                let Some(regex) = try_compile_regex(pattern, &log) else { return false; };

                let start_idx = *cur_buffer.primary_cursor().sel().start();
                let end_idx = *cur_buffer.primary_cursor().sel().end() + 1;

                let slice = cur_buffer.slice_clamped(start_idx, end_idx);
                let initial_cursor = cur_buffer.primary_cursor;

                let ranges: Vec<_> = regex
                    .find_iter(rope_input(slice))
                    .map(|m| m.start()..=m.end().saturating_sub(1))
                    .collect();

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
