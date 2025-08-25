use crate::*;
use kerbin_macros::Command;
use ropey::{LineType, Rope};

#[derive(Debug, Clone, Command)]
pub enum MotionCommand {
    #[command(drop_ident, name = "sel_word", name = "sw")]
    SelectWord { extend: bool },
    #[command(drop_ident, name = "sel_back_word", name = "sb")]
    SelectBackWord { extend: bool },
    #[command(drop_ident, name = "sel_end_word", name = "se")]
    SelectEndOfWord { extend: bool },
    #[command(drop_ident, name = "sel_WORD", name = "sW")]
    SelectWORD { extend: bool },
    #[command(drop_ident, name = "sel_back_WORD", name = "sB")]
    SelectBackWORD { extend: bool },
    #[command(drop_ident, name = "sel_end_WORD", name = "sE")]
    SelectEndOfWORD { extend: bool },
    #[command(drop_ident, name = "sel_line", name = "sl")]
    SelectLine { extend: bool },

    #[command(drop_ident, name = "sel_mv_start", name = "sms")]
    MoveSelectionStart { by: isize },

    #[command(drop_ident, name = "sel_mv_end", name = "sme")]
    MoveSelectionEnd { by: isize },

    #[command(drop_ident, name = "sel_clear", name = "sc")]
    ClearSelection,

    #[command(drop_ident, name = "go_sel_end", name = "gse")]
    GotoSelectionEnd,
    #[command(drop_ident, name = "go_sel_begin", name = "gsb")]
    GotoSelectionBegin,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[allow(non_snake_case)]
fn is_WORD_char(c: char) -> bool {
    !c.is_whitespace()
}

// Finds the global char index of the start of the next word/WORD.
// Mimics Vim's 'w' / 'W' motion.
fn find_next_word_start(
    rope: &Rope,
    start_char_idx: usize,
    is_char_boundary: impl Fn(char) -> bool,
) -> usize {
    let len_chars = rope.len_chars();

    if start_char_idx >= len_chars {
        return len_chars; // At or beyond end of rope
    }

    let mut i = start_char_idx;

    // If currently on a char of the target type, skip to the end of this block.
    if is_char_boundary(rope.char(i)) {
        while i < len_chars && is_char_boundary(rope.char(i)) {
            i += 1;
        }
    }
    // If currently on a non-target char block (but not whitespace for 'word' type), skip it.
    else if !rope.char(i).is_whitespace() {
        while i < len_chars && !rope.char(i).is_whitespace() && !is_char_boundary(rope.char(i)) {
            i += 1;
        }
    }

    // Skip any whitespace after the current block.
    while i < len_chars && rope.char(i).is_whitespace() {
        i += 1;
    }

    i // This will be the global char index of the next word's start
}

// Finds the global char index of the start of the previous word/WORD.
// Mimics Vim's 'b' / 'B' motion.
fn find_prev_word_start(
    rope: &Rope,
    mut start_char_idx: usize,
    is_char_boundary: impl Fn(char) -> bool,
) -> usize {
    let len_chars = rope.len_chars();

    if start_char_idx == 0 {
        return 0;
    }
    if start_char_idx > len_chars {
        start_char_idx = len_chars; // Clamp to end of rope
    }

    let mut i = start_char_idx.saturating_sub(1); // Start search from char before cursor

    // Skip any trailing whitespace before the previous word/WORD.
    if rope.char(i).is_whitespace() {
        while i > 0 && rope.char(i).is_whitespace() {
            i = i.saturating_sub(1);
        }
    }

    // Now 'i' is on a non-whitespace character (or at 0).
    // Find the beginning of this word/WORD block.
    if is_char_boundary(rope.char(i)) {
        while i > 0 && is_char_boundary(rope.char(i.saturating_sub(1))) {
            i = i.saturating_sub(1);
        }
    } else if !rope.char(i).is_whitespace() {
        while i > 0
            && !rope.char(i.saturating_sub(1)).is_whitespace()
            && !is_char_boundary(rope.char(i.saturating_sub(1)))
        {
            i = i.saturating_sub(1);
        }
    }

    i // This will be the global char index of the previous word's start
}

// Finds the global char index of the end of the current or next word/WORD (inclusive).
// Mimics Vim's 'e' / 'E' motion.
fn find_next_word_end(
    rope: &Rope,
    start_char_idx: usize,
    is_char_boundary: impl Fn(char) -> bool,
) -> usize {
    let len_chars = rope.len_chars();

    if start_char_idx >= len_chars {
        // If at or beyond end, point to the last valid character or the very end if rope is empty
        return len_chars.saturating_sub(1);
    }

    let mut i = start_char_idx;

    // If currently on whitespace, skip it to find the start of a word/WORD.
    if rope.char(i).is_whitespace() {
        while i < len_chars && rope.char(i).is_whitespace() {
            i += 1;
        }
        if i >= len_chars {
            return len_chars.saturating_sub(1); // Reached end after skipping whitespace
        }
    }

    // Now 'i' is on a non-whitespace character. Find the end of this block.
    if is_char_boundary(rope.char(i)) {
        while i < len_chars && is_char_boundary(rope.char(i)) {
            i += 1;
        }
    } else {
        while i < len_chars && !rope.char(i).is_whitespace() && !is_char_boundary(rope.char(i)) {
            i += 1;
        }
    }

    i.saturating_sub(1) // Return the index of the last character of the found word/WORD
}

impl Command for MotionCommand {
    fn apply(&self, state: std::sync::Arc<State>) -> bool {
        let buffers = state.buffers.read().unwrap();
        let cur_buffer = buffers.cur_buffer();
        let mut cur_buffer = cur_buffer.write().unwrap();

        match self {
            Self::ClearSelection => {
                cur_buffer.selection = None;
                return true;
            }
            Self::GotoSelectionEnd => {
                if let Some(range) = &cur_buffer.selection {
                    cur_buffer.cursor = range.end;
                }
                return true;
            }
            Self::GotoSelectionBegin => {
                if let Some(range) = &cur_buffer.selection {
                    cur_buffer.cursor = range.start;
                }
                return true;
            }
            Self::MoveSelectionStart { by } => {
                if let Some(mut range) = cur_buffer.selection.clone() {
                    let old_start = range.start;
                    let new_start = old_start.saturating_add_signed(*by);
                    range.start = new_start.min(range.end);
                    if cur_buffer.cursor == old_start {
                        cur_buffer.cursor = range.start;
                    }

                    cur_buffer.selection = Some(range);
                }
                return true;
            }
            Self::MoveSelectionEnd { by } => {
                if let Some(mut range) = cur_buffer.selection.clone() {
                    let old_end = range.end;
                    let new_end = old_end.saturating_add_signed(*by);
                    range.end = new_end.max(range.start).min(cur_buffer.rope.len());
                    if cur_buffer.cursor == old_end {
                        cur_buffer.cursor = range.end;
                    }

                    cur_buffer.selection = Some(range);
                }
                return true;
            }
            _ => {}
        }

        if let MotionCommand::SelectLine { extend } = self {
            let line_idx = cur_buffer
                .rope
                .byte_to_line_idx(cur_buffer.cursor, LineType::LF);
            let start_byte = cur_buffer.rope.line_to_byte_idx(line_idx, LineType::LF);
            let end_byte = if line_idx >= cur_buffer.rope.len_lines(LineType::LF) - 1 {
                cur_buffer.rope.len()
            } else {
                cur_buffer.rope.line_to_byte_idx(line_idx + 1, LineType::LF)
            };

            let new_cursor_byte = end_byte;
            let (selection_start_byte, selection_end_byte) = if *extend {
                if let Some(existing_range) = cur_buffer.selection.clone() {
                    let anchor_byte = if cur_buffer.cursor == existing_range.start {
                        existing_range.end
                    } else {
                        existing_range.start
                    };
                    if new_cursor_byte > anchor_byte {
                        (anchor_byte, new_cursor_byte)
                    } else {
                        (new_cursor_byte, anchor_byte)
                    }
                } else {
                    (start_byte, end_byte)
                }
            } else {
                (start_byte, end_byte)
            };

            cur_buffer.cursor = new_cursor_byte;
            cur_buffer.selection = Some(selection_start_byte..selection_end_byte);

            return true;
        }

        let current_char_idx = cur_buffer.rope.byte_to_char_idx(cur_buffer.cursor);

        let (extend, target_char_start, target_char_end, moves_forward) = match *self {
            MotionCommand::SelectWord { extend } => (
                extend,
                current_char_idx,
                find_next_word_start(&cur_buffer.rope, current_char_idx, is_word_char),
                true,
            ),
            MotionCommand::SelectBackWord { extend } => (
                extend,
                find_prev_word_start(&cur_buffer.rope, current_char_idx, is_word_char),
                current_char_idx,
                false,
            ),
            MotionCommand::SelectEndOfWord { extend } => {
                let end = find_next_word_end(&cur_buffer.rope, current_char_idx, is_word_char);
                (extend, current_char_idx, end.saturating_add(1), true)
            }
            MotionCommand::SelectWORD { extend } => (
                extend,
                current_char_idx,
                find_next_word_start(&cur_buffer.rope, current_char_idx, is_WORD_char),
                true,
            ),
            MotionCommand::SelectBackWORD { extend } => (
                extend,
                find_prev_word_start(&cur_buffer.rope, current_char_idx, is_WORD_char),
                current_char_idx,
                false,
            ),
            MotionCommand::SelectEndOfWORD { extend } => {
                let end = find_next_word_end(&cur_buffer.rope, current_char_idx, is_WORD_char);
                (extend, current_char_idx, end.saturating_add(1), true)
            }
            _ => unreachable!(),
        };

        let new_cursor_char_idx = if moves_forward {
            target_char_end
        } else {
            target_char_start
        };
        let new_cursor_byte = cur_buffer.rope.char_to_byte_idx(new_cursor_char_idx);

        let (selection_start_byte, selection_end_byte) = if extend {
            if let Some(existing_range) = cur_buffer.selection.clone() {
                let anchor_byte = if cur_buffer.cursor == existing_range.start {
                    existing_range.end
                } else {
                    existing_range.start
                };

                if new_cursor_byte > anchor_byte {
                    (anchor_byte, new_cursor_byte)
                } else {
                    (new_cursor_byte, anchor_byte)
                }
            } else {
                // No existing selection, so just create a new one
                let start_byte = cur_buffer.rope.char_to_byte_idx(target_char_start);
                let end_byte = cur_buffer.rope.char_to_byte_idx(target_char_end);
                if start_byte > end_byte {
                    (end_byte, start_byte)
                } else {
                    (start_byte, end_byte)
                }
            }
        } else {
            let start_byte = cur_buffer.rope.char_to_byte_idx(target_char_start);
            let end_byte = cur_buffer.rope.char_to_byte_idx(target_char_end);
            if start_byte > end_byte {
                (end_byte, start_byte)
            } else {
                (start_byte, end_byte)
            }
        };

        cur_buffer.cursor = new_cursor_byte;
        let old_sel = cur_buffer.selection.clone();
        cur_buffer.selection = Some(selection_start_byte..selection_end_byte);

        old_sel != cur_buffer.selection
    }
}
