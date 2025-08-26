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
    // Also handles the case where the cursor is *inside* a word.
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
        return len_chars.saturating_sub(1);
    }

    let mut i = start_char_idx;

    if rope.char(i).is_whitespace() {
        while i < len_chars && rope.char(i).is_whitespace() {
            i += 1;
        }
        if i >= len_chars {
            return len_chars.saturating_sub(1);
        }
    }

    if is_char_boundary(rope.char(i)) {
        while i < len_chars && is_char_boundary(rope.char(i)) {
            i += 1;
        }
    } else {
        while i < len_chars && !rope.char(i).is_whitespace() && !is_char_boundary(rope.char(i)) {
            i += 1;
        }
    }

    i.saturating_sub(1)
}

impl Command for MotionCommand {
    fn apply(&self, state: std::sync::Arc<State>) -> bool {
        let buffers = state.buffers.read().unwrap();
        let cur_buffer = buffers.cur_buffer();
        let mut cur_buffer = cur_buffer.write().unwrap();

        let mut moved_any = false;
        let rope_len_bytes = cur_buffer.rope.len();
        let rope_len_lines = cur_buffer.rope.len_lines(LineType::LF_CR);

        match self {
            Self::ClearSelection => {
                for cursor_mut in cur_buffer.cursors.iter_mut() {
                    let old_sel = cursor_mut.sel().clone();
                    cursor_mut.collapse_sel();
                    if *cursor_mut.sel() != old_sel {
                        moved_any = true;
                    }
                }
                moved_any
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
            Self::MoveSelectionStart { by } => {
                for cursor_mut in cur_buffer.cursors.iter_mut() {
                    let old_sel = cursor_mut.sel().clone();
                    let old_at_start = cursor_mut.at_start();

                    let mut new_start_bound = *old_sel.start();
                    let old_end_bound = *old_sel.end();

                    new_start_bound = new_start_bound.saturating_add_signed(*by);
                    new_start_bound = new_start_bound.min(old_end_bound);
                    cursor_mut.set_sel(new_start_bound..=old_end_bound);
                    cursor_mut.set_at_start(old_at_start);

                    if *cursor_mut.sel() != old_sel {
                        moved_any = true;
                    }
                }
                moved_any
            }
            Self::MoveSelectionEnd { by } => {
                for cursor_mut in cur_buffer.cursors.iter_mut() {
                    let old_sel = cursor_mut.sel().clone();
                    let old_at_start = cursor_mut.at_start();

                    let old_start_bound = *old_sel.start();
                    let mut new_end_bound = *old_sel.end();

                    new_end_bound = new_end_bound.saturating_add_signed(*by);
                    new_end_bound = new_end_bound.max(old_start_bound).min(rope_len_bytes);

                    cursor_mut.set_sel(old_start_bound..=new_end_bound);
                    cursor_mut.set_at_start(old_at_start);

                    if *cursor_mut.sel() != old_sel {
                        moved_any = true;
                    }
                }
                moved_any
            }
            MotionCommand::SelectLine { extend } => {
                for i in 0..cur_buffer.cursors.len() {
                    let current_caret_byte = cur_buffer.cursors[i].get_cursor_byte();
                    let old_sel = cur_buffer.cursors[i].sel().clone();
                    let old_at_start = cur_buffer.cursors[i].at_start();

                    let line_idx = cur_buffer
                        .rope
                        .byte_to_line_idx(current_caret_byte, LineType::LF_CR);

                    let new_caret_byte = if line_idx + 1 >= rope_len_lines {
                        rope_len_bytes
                    } else {
                        cur_buffer
                            .rope
                            .line_to_byte_idx(line_idx + 1, LineType::LF_CR)
                    };

                    if *extend {
                        let anchor_byte = if old_at_start {
                            *old_sel.end()
                        } else {
                            *old_sel.start()
                        };

                        let start = anchor_byte.min(new_caret_byte);
                        let end = anchor_byte.max(new_caret_byte);
                        cur_buffer.cursors[i].set_sel(start..=end);
                        cur_buffer.cursors[i].set_at_start(new_caret_byte < anchor_byte);
                    } else {
                        // If not extending, collapse selection to the new caret position
                        cur_buffer.cursors[i].set_sel(new_caret_byte..=new_caret_byte);
                        cur_buffer.cursors[i].set_at_start(false);
                    }
                    if *cur_buffer.cursors[i].sel() != old_sel
                        || cur_buffer.cursors[i].at_start() != old_at_start
                    {
                        moved_any = true;
                    }
                }
                moved_any
            }

            _ => {
                for i in 0..cur_buffer.cursors.len() {
                    let current_caret_byte = cur_buffer.cursors[i].get_cursor_byte();
                    let old_sel = cur_buffer.cursors[i].sel().clone();
                    let old_at_start = cur_buffer.cursors[i].at_start();

                    let current_char_idx = cur_buffer.rope.byte_to_char_idx(current_caret_byte);

                    let new_caret_char_idx = match *self {
                        MotionCommand::SelectWord { .. } => {
                            find_next_word_start(&cur_buffer.rope, current_char_idx, is_word_char)
                        }
                        MotionCommand::SelectBackWord { .. } => {
                            find_prev_word_start(&cur_buffer.rope, current_char_idx, is_word_char)
                        }
                        MotionCommand::SelectEndOfWord { .. } => {
                            find_next_word_end(&cur_buffer.rope, current_char_idx, is_word_char)
                                .saturating_add(1)
                        }
                        MotionCommand::SelectWORD { .. } => {
                            find_next_word_start(&cur_buffer.rope, current_char_idx, is_WORD_char)
                        }
                        MotionCommand::SelectBackWORD { .. } => {
                            find_prev_word_start(&cur_buffer.rope, current_char_idx, is_WORD_char)
                        }
                        MotionCommand::SelectEndOfWORD { .. } => {
                            find_next_word_end(&cur_buffer.rope, current_char_idx, is_WORD_char)
                                .saturating_add(1)
                        }
                        _ => continue,
                    };

                    let new_caret_byte = cur_buffer.rope.char_to_byte_idx(new_caret_char_idx);

                    let extend = match *self {
                        MotionCommand::SelectWord { extend } => extend,
                        MotionCommand::SelectBackWord { extend } => extend,
                        MotionCommand::SelectEndOfWord { extend } => extend,
                        MotionCommand::SelectWORD { extend } => extend,
                        MotionCommand::SelectBackWORD { extend } => extend,
                        MotionCommand::SelectEndOfWORD { extend } => extend,
                        _ => unreachable!("All states should have been checked"),
                    };

                    if extend {
                        let anchor_byte = if old_at_start {
                            *old_sel.end()
                        } else {
                            *old_sel.start()
                        };

                        let start = anchor_byte.min(new_caret_byte);
                        let end = anchor_byte.max(new_caret_byte);
                        cur_buffer.cursors[i].set_sel(start..=end);
                        cur_buffer.cursors[i].set_at_start(new_caret_byte < anchor_byte);
                    } else {
                        let anchor_byte = if old_at_start {
                            *old_sel.start()
                        } else {
                            *old_sel.end()
                        };

                        let start = anchor_byte.min(new_caret_byte);
                        let end = anchor_byte.max(new_caret_byte);
                        cur_buffer.cursors[i].set_sel(start..=end);
                        cur_buffer.cursors[i].set_at_start(new_caret_byte < anchor_byte);
                    }

                    if *cur_buffer.cursors[i].sel() != old_sel
                        || cur_buffer.cursors[i].at_start() != old_at_start
                    {
                        moved_any = true;
                    }
                }
                moved_any
            }
        }
    }
}
