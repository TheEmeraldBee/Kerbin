use crate::*;
use kerbin_macros::Command;

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

// Finds the column index of the start of the next word/WORD.
// Mimics Vim's 'w' / 'W' motion.
fn find_next_word_start(line: &str, col: usize, is_char_boundary: impl Fn(char) -> bool) -> usize {
    let line_chars: Vec<char> = line.chars().collect();
    let len = line_chars.len();

    if col >= len {
        return len;
    }

    let mut i = col;

    // If currently on a char of the target type, skip to the end of this block.
    if is_char_boundary(line_chars[i]) {
        while i < len && is_char_boundary(line_chars[i]) {
            i += 1;
        }
    }
    // If currently on a non-target char block (but not whitespace for 'word' type), skip it.
    else if !line_chars[i].is_whitespace() {
        while i < len && !line_chars[i].is_whitespace() && !is_char_boundary(line_chars[i]) {
            i += 1;
        }
    }

    // Skip any whitespace after the current block.
    while i < len && line_chars[i].is_whitespace() {
        i += 1;
    }

    i
}

// Finds the column index of the start of the previous word/WORD.
// Mimics Vim's 'b' / 'B' motion.
fn find_prev_word_start(
    line: &str,
    mut col: usize,
    is_char_boundary: impl Fn(char) -> bool,
) -> usize {
    let line_chars: Vec<char> = line.chars().collect();
    let len = line_chars.len();

    if col == 0 {
        return 0;
    }
    if col > len {
        col = len;
    }

    let mut i = col.saturating_sub(1); // Start search from char before cursor

    // Skip any trailing whitespace before the previous word/WORD.
    if line_chars[i].is_whitespace() {
        while i > 0 && line_chars[i].is_whitespace() {
            i -= 1;
        }
    }

    // Now 'i' is on a non-whitespace character (or at 0).
    // Find the beginning of this word/WORD block.
    if is_char_boundary(line_chars[i]) {
        while i > 0 && is_char_boundary(line_chars[i - 1]) {
            i -= 1;
        }
    } else if !line_chars[i].is_whitespace() {
        while i > 0 && !line_chars[i - 1].is_whitespace() && !is_char_boundary(line_chars[i - 1]) {
            i -= 1;
        }
    }

    i
}

// Finds the column index of the end of the current or next word/WORD (inclusive).
// Mimics Vim's 'e' / 'E' motion.
fn find_next_word_end(line: &str, col: usize, is_char_boundary: impl Fn(char) -> bool) -> usize {
    let line_chars: Vec<char> = line.chars().collect();
    let len = line_chars.len();

    if col >= len {
        return len.saturating_sub(1);
    }

    let mut i = col;

    // If currently on whitespace, skip it to find the start of a word/WORD.
    if line_chars[i].is_whitespace() {
        while i < len && line_chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            return len.saturating_sub(1);
        }
    }

    // Now 'i' is on a non-whitespace character. Find the end of this block.
    if is_char_boundary(line_chars[i]) {
        while i < len && is_char_boundary(line_chars[i]) {
            i += 1;
        }
    } else {
        while i < len && !line_chars[i].is_whitespace() && !is_char_boundary(line_chars[i]) {
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

        match self {
            Self::GotoSelectionEnd => {
                let Some((row, ref range)) = cur_buffer.selection else {
                    return false;
                };

                cur_buffer.col = range.end;
                cur_buffer.row = row;

                return true;
            }
            Self::GotoSelectionBegin => {
                let Some((row, ref range)) = cur_buffer.selection else {
                    return false;
                };

                cur_buffer.col = range.start;
                cur_buffer.row = row;

                return true;
            }
            _ => {}
        }

        let (row, col) = (cur_buffer.row, cur_buffer.col);
        let line = cur_buffer.cur_line();

        if line.is_empty() {
            return true;
        }

        let (start, end) = match self {
            MotionCommand::SelectWord => {
                let end = find_next_word_start(&line, col, is_word_char);
                (col, end)
            }
            MotionCommand::SelectBackWord => {
                let start = find_prev_word_start(&line, col, is_word_char);
                (start, col)
            }
            MotionCommand::SelectEndOfWord => {
                let end = find_next_word_end(&line, col, is_word_char);
                // In vim 'e' motion is inclusive, so selection goes up to and includes that char
                (col, end.saturating_add(1))
            }
            MotionCommand::SelectWORD => {
                let end = find_next_word_start(&line, col, is_WORD_char);
                (col, end)
            }
            MotionCommand::SelectBackWORD => {
                let start = find_prev_word_start(&line, col, is_WORD_char);
                (start, col)
            }
            MotionCommand::SelectEndOfWORD => {
                let end = find_next_word_end(&line, col, is_WORD_char);
                // In vim 'E' motion is inclusive, so selection goes up to and includes that char
                (col, end.saturating_add(1))
            }
            _ => unreachable!(),
        };

        cur_buffer.col = end;
        cur_buffer.selection = Some((row, start..end));

        true
    }
}
