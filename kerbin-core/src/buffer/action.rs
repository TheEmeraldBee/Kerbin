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
    pub byte: usize,

    pub content: String,
}

impl BufferAction for Insert {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.byte > buf.rope.len() {
            return ActionResult::none(false);
        }

        let start = buf.get_edit_part(self.byte);

        buf.rope.insert(self.byte, &self.content);

        let content_len = self.content.len();
        for (i, cursor) in buf.cursors.iter_mut().enumerate() {
            if i == buf.primary_cursor {
                continue;
            }
            let start_byte = *cursor.sel.start();
            let end_byte = *cursor.sel.end();

            if start_byte > self.byte {
                cursor.sel = (start_byte + content_len)..=(end_byte + content_len);
            } else if end_byte >= self.byte {
                cursor.sel = start_byte..=(end_byte + content_len);
            }
        }

        let end = buf.get_edit_part(self.byte + self.content.len());

        buf.register_input_edit(start, start, end);

        let inverse = Box::new(Delete {
            byte: self.byte,
            len: self.content.len(),
        });

        ActionResult::new(true, inverse)
    }
}

pub struct Delete {
    /// Byte location of the edit (can be gotten from `buf.translate_coords(x, y)`).
    pub byte: usize,

    /// Length in **bytes**
    pub len: usize,
}

impl BufferAction for Delete {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.byte + self.len > buf.rope.len() {
            return ActionResult::none(false);
        }

        let start = buf.get_edit_part(self.byte);
        let old_end = buf.get_edit_part(self.byte + self.len);

        let removed = buf
            .rope
            .slice(self.byte..(self.byte + self.len))
            .to_string();

        buf.rope.remove(self.byte..(self.byte + self.len));

        for (i, cursor) in buf.cursors.iter_mut().enumerate() {
            if i == buf.primary_cursor {
                continue;
            }

            let start_byte = *cursor.sel.start();
            let end_byte = *cursor.sel.end();
            let mut new_start = start_byte;
            let mut new_end = end_byte;

            if start_byte >= self.byte + self.len {
                new_start = start_byte.saturating_sub(self.len);
            } else if start_byte >= self.byte {
                new_start = self.byte;
            }

            if end_byte >= self.byte + self.len {
                new_end = end_byte.saturating_sub(self.len);
            } else if end_byte >= self.byte {
                new_end = self.byte;
            }

            cursor.sel = new_start..=new_end;
        }

        buf.register_input_edit(start, old_end, start);

        let inverse = Box::new(Insert {
            byte: self.byte,
            content: removed,
        });

        ActionResult::new(true, inverse)
    }
}

// pub struct JoinLine {
//     pub row: usize,
//     pub undo_indent: Option<usize>,
// }

// impl BufferAction for JoinLine {
//     extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
//         if self.row + 1 >= buf.lines.len() {
//             return ActionResult::none(false);
//         }

//         if let Some(indent_len_bytes) = self.undo_indent {
//             let line0_char_len = buf.lines[self.row].chars().count();
//             let start = buf.get_edit_part(self.row, line0_char_len);

//             let line1_ref = &buf.lines[self.row + 1];
//             let prefix_len = indent_len_bytes.min(line1_ref.len());
//             let prefix_char_len = line1_ref[..prefix_len].chars().count();

//             let old_end = buf.get_edit_part(self.row + 1, prefix_char_len);

//             let mut line1_content = buf.lines.remove(self.row + 1);
//             line1_content.drain(..prefix_len);

//             let new_end = start;

//             buf.lines[self.row].push_str(&line1_content);
//             buf.register_input_edit(start, old_end, new_end);

//             ActionResult::new(
//                 true,
//                 Box::new(InsertNewline {
//                     row: self.row,
//                     col: line0_char_len,
//                 }),
//             )
//         } else {
//             let line0 = &buf.lines[self.row];
//             let line1 = &buf.lines[self.row + 1];

//             let trimmed_line0 = line0.trim_end();
//             let trimmed_line1 = line1.trim_start();

//             let start_char_col = trimmed_line0.chars().count();
//             let start = buf.get_edit_part(self.row, start_char_col);

//             let leading_ws_len_chars = line1.chars().count() - trimmed_line1.chars().count();
//             let old_end = buf.get_edit_part(self.row + 1, leading_ws_len_chars);

//             let space = if !trimmed_line0.is_empty() && !trimmed_line1.is_empty() {
//                 " "
//             } else {
//                 ""
//             };

//             let mut new_line = trimmed_line0.to_string();
//             new_line.push_str(space);
//             let inverse_col = new_line.chars().count();
//             new_line.push_str(trimmed_line1);

//             let new_end = buf.get_edit_part(self.row, start_char_col + space.chars().count());

//             buf.lines[self.row] = new_line;
//             buf.lines.remove(self.row + 1);

//             buf.register_input_edit(start, old_end, new_end);

//             ActionResult::new(
//                 true,
//                 Box::new(InsertNewline {
//                     row: self.row,
//                     col: inverse_col,
//                 }),
//             )
//         }
//     }
// }

// pub struct InsertNewline {
//     pub row: usize,
//     pub col: usize,
// }

// impl BufferAction for InsertNewline {
//     extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
//         if self.row >= buf.lines.len() {
//             return ActionResult::none(false);
//         }

//         let start = buf.get_edit_part(self.row, self.col);

//         let current_line = &buf.lines[self.row];
//         let byte_col = char_to_byte_index(current_line, self.col);

//         let indent = current_line
//             .chars()
//             .take_while(|c| c.is_whitespace())
//             .collect::<String>();

//         let (lhs, rhs) = current_line.split_at(byte_col);
//         let (lhs, rhs) = (lhs.to_string(), format!("{indent}{rhs}"));
//         buf.lines[self.row] = lhs.clone();
//         buf.lines.insert(self.row + 1, rhs);

//         // For Tree-sitter, we need to consider the newline character that was inserted.
//         // It's conceptually at the end of the line.
//         let byte_len_of_inserted_newline = match buf.line_ending_style {
//             crate::LineEnding::LF | crate::LineEnding::CR => 1,
//             crate::LineEnding::CRLF => 2,
//             _ => 1, // Default for Mixed or None
//         };

//         let new_end_point = Point::new(self.row + 1, 0); // Point after newline
//         let new_end_byte = start.1 + (lhs.len() - byte_col) + byte_len_of_inserted_newline;

//         buf.register_input_edit(start, start, (new_end_point, new_end_byte));

//         buf.move_cursor(1, 0);
//         buf.col = indent.chars().count();

//         ActionResult::new(
//             true,
//             Box::new(JoinLine {
//                 row: self.row,
//                 undo_indent: Some(indent.len()),
//             }),
//         )
//     }
// }

// #[derive(Clone)]
// pub struct DeleteLine {
//     pub row: usize,
// }

// impl BufferAction for DeleteLine {
//     extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
//         if self.row >= buf.lines.len() {
//             return ActionResult::none(false);
//         }

//         let is_last_line = self.row == buf.lines.len() - 1;
//         let was_last_and_not_only_line = is_last_line && self.row > 0;

//         let start_edit;
//         let old_end_edit;

//         if was_last_and_not_only_line {
//             let prev_row = self.row - 1;
//             let prev_line_char_len = buf.lines[prev_row].chars().count();
//             start_edit = buf.get_edit_part(prev_row, prev_line_char_len);

//             // The old_end is the end of the line being deleted + its implicit newline
//             old_end_edit = buf.get_edit_part(self.row, buf.lines[self.row].chars().count());
//         } else {
//             start_edit = buf.get_edit_part(self.row, 0);
//             old_end_edit = if !is_last_line {
//                 // Deleting a line in the middle: removed line + its newline + the newline of the line BEFORE it.
//                 // Or simply: the start of the next line (conceptually).
//                 buf.get_edit_part(self.row + 1, 0)
//             } else {
//                 // Deleting the very last line (and it's the only line or the only remaining line)
//                 // Remove content, no trailing newline.
//                 let current_line_char_len = buf.lines[self.row].chars().count();
//                 buf.get_edit_part(self.row, current_line_char_len)
//             };
//         }

//         let removed = buf.lines.remove(self.row);

//         if buf.lines.is_empty() {
//             buf.lines.push(String::new());
//         }

//         buf.register_input_edit(start_edit, old_end_edit, start_edit);
//         buf.move_cursor(0, 0);

//         let inverse = InsertLine {
//             row: self.row,
//             content: removed,
//             was_last_line: was_last_and_not_only_line,
//         };

//         ActionResult::new(true, Box::new(inverse))
//     }
// }

// pub struct InsertLine {
//     pub row: usize,
//     pub content: String,
//     pub was_last_line: bool, // indicates if this line was originally the last line of the file (before being deleted)
// }

// impl BufferAction for InsertLine {
//     extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
//         if self.row > buf.lines.len() {
//             return ActionResult::none(false);
//         }

//         let start_edit;
//         let new_end_edit;

//         if self.was_last_line {
//             // Inserting a line that was previously the last line
//             let prev_row = self.row.saturating_sub(1);
//             let prev_line_char_len = if prev_row < buf.lines.len() {
//                 buf.lines[prev_row].chars().count()
//             } else {
//                 0
//             };
//             start_edit = buf.get_edit_part(prev_row, prev_line_char_len);

//             buf.lines.insert(self.row, self.content.clone());

//             let new_line_char_len = buf.lines[self.row].chars().count();
//             new_end_edit = buf.get_edit_part(self.row, new_line_char_len);
//         } else {
//             // Inserting a line normally (in the middle or at the beginning of the file)
//             start_edit = buf.get_edit_part(self.row, 0);

//             buf.lines.insert(self.row, self.content.clone());

//             new_end_edit = buf.get_edit_part(self.row + 1, 0);
//         }

//         buf.register_input_edit(start_edit, start_edit, new_end_edit);

//         buf.row = self.row;
//         buf.col = 0;

//         ActionResult::new(true, Box::new(DeleteLine { row: self.row }))
//     }
// }

pub struct NoOp;
impl BufferAction for NoOp {
    extern "C" fn apply(&self, _buf: &mut TextBuffer) -> ActionResult {
        ActionResult::none(true)
    }
}
