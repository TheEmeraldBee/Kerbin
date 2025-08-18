use crate::char_to_byte_index;

use super::TextBuffer;

pub struct ActionResult {
    pub success: bool,
    pub action: Box<dyn BufferAction>,
}

impl ActionResult {
    pub fn new(success: bool, action: Box<dyn BufferAction>) -> Self {
        Self { success, action }
    }

    pub fn none(success: bool) -> Self {
        Self::new(success, Box::new(NoOp))
    }
}

pub trait BufferAction: Send + Sync {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult;
}

pub struct Insert {
    pub row: usize,
    pub col: usize,

    pub content: String,
}

impl BufferAction for Insert {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.row > buf.lines.len() {
            ActionResult::none(false)
        } else {
            let start = buf.get_edit_part(self.row, self.col);

            buf.lines[self.row].insert_str(self.col, &self.content);

            let end = buf.get_edit_part(self.row, self.col + self.content.len());

            buf.register_input_edit(start, start, end);

            let inverse = Box::new(Delete {
                row: self.row,
                col: self.col,

                len: self.content.chars().count(),
            });

            ActionResult::new(true, inverse)
        }
    }
}

pub struct Delete {
    pub row: usize,
    pub col: usize,

    pub len: usize,
}

impl BufferAction for Delete {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.row > buf.lines.len() {
            ActionResult::none(false)
        } else {
            let end = self
                .col
                .saturating_add(self.len)
                .min(buf.lines[self.row].chars().count());

            let start_edit = buf.get_edit_part(self.row, self.col);
            let end_edit = buf.get_edit_part(self.row, end);

            // Remove the chars from the string
            let removed: String = buf.lines[self.row].drain(self.col..end).collect::<String>();

            buf.register_input_edit(start_edit, end_edit, start_edit);

            let inverse = Box::new(Insert {
                row: self.row,
                col: self.col,
                content: removed,
            });

            ActionResult::new(true, inverse)
        }
    }
}

pub struct JoinLine {
    pub row: usize,
    pub undo_indent: Option<usize>,
}

impl BufferAction for JoinLine {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.row + 1 > buf.lines.len() {
            return ActionResult::none(false);
        }

        let mut line1_content = buf.lines[self.row + 1].clone();

        if let Some(indent_len) = self.undo_indent {
            line1_content.drain(..indent_len);
        } else {
            line1_content = line1_content.trim_start().to_string();
        }

        let line1_len_bytes = line1_content.len();

        let line0 = &mut buf.lines[self.row];
        if self.undo_indent.is_none() {
            let line0_trimmed_len = line0.trim_end().len();
            line0.truncate(line0_trimmed_len);
            line0.push(' ');
        }

        let line0_len_bytes = line0.len();
        let line0_len_chars = line0.chars().count();

        let start = buf.get_edit_part(self.row, line0_len_bytes);
        let old_end = buf.get_edit_part(self.row + 1, line1_len_bytes);

        let new_end = buf.get_edit_part(self.row, line0_len_bytes + line1_len_bytes);

        buf.lines.remove(self.row + 1);
        buf.lines[self.row].push_str(&line1_content);

        buf.register_input_edit(start, old_end, new_end);

        buf.move_cursor(0, 0);

        ActionResult::new(
            true,
            Box::new(InsertNewline {
                row: self.row,
                col: line0_len_chars,
            }),
        )
    }
}

pub struct InsertNewline {
    pub row: usize,
    pub col: usize,
}

impl BufferAction for InsertNewline {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.row > buf.lines.len() {
            return ActionResult::none(false);
        }

        let start = buf.get_edit_part(self.row, self.col);

        let byte_col = char_to_byte_index(&buf.lines[self.row], self.col);

        let current_line = &buf.lines[self.row];
        let indent = current_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        let (lhs, rhs) = current_line.split_at(byte_col);
        let (lhs, rhs) = (lhs.to_string(), format!("{indent}{rhs}"));
        buf.lines[self.row] = lhs;
        buf.lines.insert(self.row + 1, rhs);

        let new_end = buf.get_edit_part(self.row + 1, 0);

        buf.register_input_edit(start, start, new_end);

        buf.move_cursor(1, 0);
        buf.col = indent.len();

        ActionResult::new(
            true,
            Box::new(JoinLine {
                row: self.row,
                undo_indent: Some(indent.len()),
            }),
        )
    }
}

pub struct NoOp;
impl BufferAction for NoOp {
    extern "C" fn apply(&self, _buf: &mut TextBuffer) -> ActionResult {
        ActionResult::none(true)
    }
}
