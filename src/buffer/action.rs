use crate::buffer::{TextBuffer, char_to_byte_index};
use ascii_forge::prelude::*;
use tree_sitter::{InputEdit, Point};

pub trait BufferAction {
    fn apply(&self, buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>);
}

#[derive(Clone)]
pub struct Insert {
    pub pos: Vec2,
    pub content: String,
}
impl BufferAction for Insert {
    fn apply(&self, buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>) {
        let line_idx = self.pos.y as usize;
        if let Some(line) = buf.lines.get_mut(line_idx) {
            let char_col = self.pos.x as usize;

            line.insert_str(char_col, &self.content);

            let byte_col = char_to_byte_index(line, char_col);
            let start_pos = Point::new(line_idx, byte_col);
            let start_byte = buf.get_byte_offset(start_pos);

            let new_end_byte_col = byte_col + self.content.len();
            let edit = InputEdit {
                start_byte,
                old_end_byte: start_byte,
                new_end_byte: start_byte + self.content.len(),
                start_position: start_pos,
                old_end_position: start_pos,
                new_end_position: Point::new(line_idx, new_end_byte_col),
            };
            buf.changes.push(edit);
            buf.tree_sitter_dirty = true;

            let inverse = Box::new(Delete {
                pos: self.pos,
                len: self.content.chars().count(),
            });
            (true, inverse)
        } else {
            (false, Box::new(NoOp))
        }
    }
}

#[derive(Clone)]
pub struct Delete {
    pub pos: Vec2,
    pub len: usize,
}
impl BufferAction for Delete {
    fn apply(&self, buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>) {
        let line_idx = self.pos.y as usize;
        if let Some(line) = buf.lines.get_mut(line_idx) {
            let start_char = self.pos.x as usize;
            let end_char = start_char
                .saturating_add(self.len)
                .min(line.chars().count());

            let start_byte_col = char_to_byte_index(line, start_char);
            let end_byte_col = char_to_byte_index(line, end_char);

            let removed: String = line.drain(start_char..end_char).collect();

            buf.move_cursor(0, 0);

            let start_pos = Point::new(line_idx, start_byte_col);
            let start_byte = buf.get_byte_offset(start_pos);
            let old_end_pos = Point::new(line_idx, end_byte_col);
            let old_end_byte = buf.get_byte_offset(old_end_pos);

            let edit = InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte: start_byte,
                start_position: start_pos,
                old_end_position: old_end_pos,
                new_end_position: start_pos,
            };
            buf.changes.push(edit);
            buf.tree_sitter_dirty = true;

            let inverse = Box::new(Insert {
                pos: self.pos,
                content: removed,
            });
            (true, inverse)
        } else {
            (false, Box::new(NoOp))
        }
    }
}

#[derive(Clone)]
pub struct JoinLine {
    /// The index of the first line (the one that will be kept).
    pub line_idx: usize,
    pub undo_indent: Option<usize>,
}

impl BufferAction for JoinLine {
    fn apply(&self, buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>) {
        if self.line_idx + 1 >= buf.lines.len() {
            return (false, Box::new(NoOp));
        }

        let mut line1_content = buf.lines[self.line_idx + 1].clone();
        if let Some(indent_len) = self.undo_indent {
            line1_content.drain(..indent_len);
        } else {
            line1_content = line1_content.trim_start().to_string();
        }
        let line1_len_bytes = line1_content.len();

        let line0 = &mut buf.lines[self.line_idx];
        if self.undo_indent.is_none() {
            let line0_trimmed_len = line0.trim_end().len();
            line0.truncate(line0_trimmed_len);
            line0.push(' ');
        }

        let line0_len_chars = line0.chars().count();
        let line0_len_bytes = line0.len();

        let start_pos = Point::new(self.line_idx, line0_len_bytes);
        let start_byte = buf.get_byte_offset(start_pos);

        let old_end_byte = start_byte + 1 + line1_len_bytes;
        let old_end_pos = Point::new(self.line_idx + 1, line1_len_bytes);

        let new_end_byte = start_byte + line1_content.len();
        let new_end_pos = Point::new(self.line_idx, line0_len_bytes + line1_content.len());

        buf.lines.remove(self.line_idx + 1);
        buf.lines[self.line_idx].push_str(&line1_content);

        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: start_pos,
            old_end_position: old_end_pos,
            new_end_position: new_end_pos,
        };
        buf.changes.push(edit);
        buf.tree_sitter_dirty = true;

        buf.move_cursor(0, 0);

        let inverse = Box::new(InsertNewline {
            pos: vec2(line0_len_chars as u16, self.line_idx as u16),
        });

        (true, inverse)
    }
}

#[derive(Clone)]
pub struct InsertNewline {
    pub pos: Vec2,
}
impl BufferAction for InsertNewline {
    fn apply(&self, buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>) {
        let line_idx = self.pos.y as usize;
        let char_col = self.pos.x as usize;
        let byte_col = char_to_byte_index(&buf.lines[line_idx], char_col);
        let start_pos = Point::new(line_idx, byte_col);
        let start_byte = buf.get_byte_offset(start_pos);

        let current_line = &buf.lines[line_idx];
        let indent = current_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        let (lhs, rhs) = current_line.split_at(byte_col);
        let (lhs, rhs) = (lhs.to_string(), format!("{indent}{rhs}"));
        buf.lines[line_idx] = lhs;
        buf.lines.insert(line_idx + 1, rhs);

        let edit = InputEdit {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte: start_byte + 1,
            start_position: start_pos,
            old_end_position: start_pos,
            new_end_position: Point::new(line_idx + 1, 0),
        };
        buf.changes.push(edit);
        buf.tree_sitter_dirty = true;

        buf.move_cursor(0, 0);
        buf.move_cursor(32000, 0);
        buf.move_cursor(-32000, 1);
        buf.move_cursor(indent.len() as i16, 0);

        let inverse = Box::new(JoinLine {
            line_idx,
            undo_indent: Some(indent.len()),
        });
        (true, inverse)
    }
}

#[derive(Clone)]
pub struct DeleteLine {
    pub line_idx: usize,
}
impl BufferAction for DeleteLine {
    fn apply(&self, buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>) {
        if self.line_idx >= buf.lines.len() {
            return (false, Box::new(NoOp));
        }

        let start_pos = Point::new(self.line_idx, 0);
        let start_byte = buf.get_byte_offset(start_pos);
        let removed_len = buf.lines[self.line_idx].len() + 1;

        let removed = buf.lines.remove(self.line_idx);

        let edit = InputEdit {
            start_byte,
            old_end_byte: start_byte + removed_len,
            new_end_byte: start_byte,
            start_position: start_pos,
            old_end_position: Point::new(self.line_idx + 1, 0),
            new_end_position: start_pos,
        };
        buf.changes.push(edit);
        buf.tree_sitter_dirty = true;

        buf.move_cursor(0, 0);

        let inverse = Box::new(InsertLine {
            line_idx: self.line_idx,
            content: removed,
        });

        if buf.lines.is_empty() {
            buf.lines.push(String::new());
            return (false, inverse);
        }

        (true, inverse)
    }
}

#[derive(Clone)]
pub struct InsertLine {
    pub line_idx: usize,
    pub content: String,
}
impl BufferAction for InsertLine {
    fn apply(&self, buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>) {
        let edit_line_row = if self.line_idx == 0 {
            0
        } else {
            self.line_idx - 1
        };
        let edit_line_len = buf.lines[edit_line_row].len();
        let start_pos = Point::new(edit_line_row, edit_line_len);
        let start_byte = buf.get_byte_offset(start_pos);

        let current_line = &buf.lines[edit_line_row];
        let indent = current_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        buf.lines
            .insert(self.line_idx, format!("{indent}{}", self.content));

        let new_end_pos = Point::new(self.line_idx, 0);
        let edit = InputEdit {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte: start_byte + 1,
            start_position: start_pos,
            old_end_position: start_pos,
            new_end_position: new_end_pos,
        };
        buf.changes.push(edit);
        buf.tree_sitter_dirty = true;

        buf.move_cursor(32000, 0);
        buf.move_cursor(-32000, 0);
        buf.move_cursor(indent.len() as i16, 0);

        let inverse = Box::new(DeleteLine {
            line_idx: self.line_idx,
        });
        (true, inverse)
    }
}

/// A no-op action, used for fallbacks.
pub struct NoOp;
impl BufferAction for NoOp {
    fn apply(&self, _buf: &mut TextBuffer) -> (bool, Box<dyn BufferAction>) {
        (true, Box::new(NoOp))
    }
}
