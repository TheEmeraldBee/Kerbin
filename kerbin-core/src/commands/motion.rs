use crate::*;
use kerbin_macros::Command;
use ropey::{LineType, Rope};

#[derive(Debug, Clone, Command)]
pub enum MotionCommand {
    #[command(drop_ident, name = "sel_word", name = "sw")]
    /// Selects the next word
    /// Extends selection if extend is set
    SelectWord { extend: bool },
    #[command(drop_ident, name = "sel_back_word", name = "sb")]
    /// Selects the previous word
    /// Extends selection if extend is set
    SelectBackWord { extend: bool },
    #[command(drop_ident, name = "sel_end_word", name = "se")]
    /// Selects to the end of the word
    /// Extends selection if extend is set
    SelectEndOfWord { extend: bool },
    #[command(drop_ident, name = "sel_WORD", name = "sW")]
    /// Selects the next WORD
    /// Extends selection if extend is set
    SelectWORD { extend: bool },
    #[command(drop_ident, name = "sel_back_WORD", name = "sB")]
    /// Selects the previous WORD
    /// Extends selection if extend is set
    SelectBackWORD { extend: bool },
    #[command(drop_ident, name = "sel_end_WORD", name = "sE")]
    /// Selects to the end of the next WORD
    /// Extends selection if extend is set
    SelectEndOfWORD { extend: bool },
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

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[allow(non_snake_case)]
fn is_WORD_char(c: char) -> bool {
    !c.is_whitespace()
}

fn find_next_word_start(
    rope: &Rope,
    start_char_idx: usize,
    is_char_boundary: impl Fn(char) -> bool,
) -> usize {
    let len_chars = rope.len_chars();

    if start_char_idx >= len_chars {
        return len_chars;
    }

    let mut i = start_char_idx;

    if is_char_boundary(rope.char(i)) {
        while i < len_chars && is_char_boundary(rope.char(i)) {
            i += 1;
        }
    } else if !rope.char(i).is_whitespace() {
        while i < len_chars && !rope.char(i).is_whitespace() && !is_char_boundary(rope.char(i)) {
            i += 1;
        }
    }

    while i < len_chars && rope.char(i).is_whitespace() {
        i += 1;
    }

    i
}

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
        start_char_idx = len_chars;
    }

    let mut i = start_char_idx.saturating_sub(1);

    if rope.char(i).is_whitespace() {
        while i > 0 && rope.char(i).is_whitespace() {
            i = i.saturating_sub(1);
        }
    }

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

    i
}

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

                let line_idx = cur_buffer
                    .rope
                    .byte_to_line_idx(current_caret_byte + 1, LineType::LF_CR);

                let new_caret_byte = if line_idx + 1 >= rope_len_lines {
                    rope_len_bytes
                } else {
                    cur_buffer
                        .rope
                        .line_to_byte_idx(line_idx + 1, LineType::LF_CR)
                        - 1
                };

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

            _ => {
                let current_caret_byte = cur_buffer.primary_cursor().get_cursor_byte();
                let old_sel = cur_buffer.primary_cursor().sel().clone();
                let old_at_start = cur_buffer.primary_cursor().at_start();

                let current_char_idx = cur_buffer.rope.byte_to_char_idx(current_caret_byte);

                let new_caret_char_idx = match *self {
                    MotionCommand::SelectWord { .. } => {
                        find_next_word_start(&cur_buffer.rope, current_char_idx + 1, is_word_char)
                            .saturating_sub(1)
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
                    _ => return false,
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

                let anchor_byte = if extend == old_at_start {
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
        }
    }
}
