use crate::*;
use kerbin_macros::Command;
use ropey::Rope;

#[derive(Debug, Clone, Command)]
pub enum MotionCommand {
    #[command(drop_ident, name = "sel_word", name = "sw")]
    SelectWord,
    #[command(drop_ident, name = "sel_back_word", name = "sb")]
    SelectBackWord,
    #[command(drop_ident, name = "sel_end_word", name = "se")]
    SelectEndOfWord,
    #[command(drop_ident, name = "sel_WORD", name = "sW")]
    SelectWORD,
    #[command(drop_ident, name = "sel_back_WORD", name = "sB")]
    SelectBackWORD,
    #[command(drop_ident, name = "sel_end_WORD", name = "sE")]
    SelectEndOfWORD,

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
            Self::GotoSelectionEnd => {
                let Some(range) = &cur_buffer.selection else {
                    return false;
                };

                cur_buffer.cursor = range.end;

                return true;
            }
            Self::GotoSelectionBegin => {
                let Some(range) = &cur_buffer.selection else {
                    return false;
                };

                cur_buffer.cursor = range.start;

                return true;
            }
            _ => {}
        }

        let rope_len_chars = cur_buffer.rope.len_chars();

        // Convert current byte cursor to global char index
        let current_char_idx = cur_buffer.rope.byte_to_char_idx(cur_buffer.cursor);

        let (mut target_char_start, mut target_char_end) = match self {
            MotionCommand::SelectWord => {
                let end = find_next_word_start(&cur_buffer.rope, current_char_idx, is_word_char);
                (current_char_idx, end)
            }
            MotionCommand::SelectBackWord => {
                let start = find_prev_word_start(&cur_buffer.rope, current_char_idx, is_word_char);
                (start, current_char_idx)
            }
            MotionCommand::SelectEndOfWord => {
                let end = find_next_word_end(&cur_buffer.rope, current_char_idx, is_word_char);
                // In vim 'e' motion is inclusive, so selection goes up to and includes that char
                // The 'end' from find_next_word_end is the last char of the word, so `+1` makes it
                // exclusive end for a range.
                (current_char_idx, end.saturating_add(1))
            }
            MotionCommand::SelectWORD => {
                let end = find_next_word_start(&cur_buffer.rope, current_char_idx, is_WORD_char);
                (current_char_idx, end)
            }
            MotionCommand::SelectBackWORD => {
                let start = find_prev_word_start(&cur_buffer.rope, current_char_idx, is_WORD_char);
                (start, current_char_idx)
            }
            MotionCommand::SelectEndOfWORD => {
                let end = find_next_word_end(&cur_buffer.rope, current_char_idx, is_WORD_char);
                // Same as above, `+1` for exclusive end.
                (current_char_idx, end.saturating_add(1))
            }
            _ => unreachable!(),
        };

        // Ensure target_char_end is not beyond the rope's character length
        target_char_end = target_char_end.min(rope_len_chars);
        target_char_start = target_char_start.min(target_char_end); // Ensure start <= end

        // Convert target char indices to byte indices for `cur_buffer.cursor` and `selection`
        let new_cursor_byte = cur_buffer.rope.char_to_byte_idx(target_char_end);
        let selection_start_byte = cur_buffer.rope.char_to_byte_idx(target_char_start);
        let selection_end_byte = cur_buffer.rope.char_to_byte_idx(target_char_end);

        cur_buffer.cursor = new_cursor_byte;
        cur_buffer.selection = Some(selection_start_byte..selection_end_byte);

        true
    }
}
