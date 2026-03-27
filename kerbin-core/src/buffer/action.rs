use super::{Cursor, TextBuffer};
use crate::buffer::text_rope_handlers::SafeRopeAccess;

fn shift_cursors(
    cursors: &mut [Cursor],
    primary: usize,
    f: impl Fn(usize, usize) -> (usize, usize),
) {
    for (i, cursor) in cursors.iter_mut().enumerate() {
        if i == primary {
            continue;
        }
        let start = *cursor.sel.start();
        let end = *cursor.sel.end();
        let (new_start, new_end) = f(start, end);
        cursor.sel = new_start..=new_end;
    }
}

/// A result of an action that stores whether the action was applied
pub struct ActionResult {
    /// `true` if the action was successfully applied, `false` otherwise
    pub success: bool,
    /// A boxed `BufferAction` representing the inverse of the applied action
    pub action: Box<dyn BufferAction>,
}

impl ActionResult {
    pub fn new(success: bool, action: Box<dyn BufferAction>) -> Self {
        Self { success, action }
    }

    /// Creates an `ActionResult` with a `NoOp` inverse action
    pub fn none(success: bool) -> Self {
        Self::new(success, Box::new(NoOp))
    }
}

/// Represents a reversible change to a `TextBuffer`
pub trait BufferAction: Send + Sync {
    fn apply(&self, buf: &mut TextBuffer) -> ActionResult;
}

/// Inserts text at a given byte offset
pub struct Insert {
    pub byte: usize,
    pub content: String,
}

impl BufferAction for Insert {
    fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.byte > buf.len() {
            return ActionResult::none(false);
        }

        let actual_byte = buf.byte_to_char_boundary(self.byte);
        let start = buf.get_edit_part(actual_byte);

        buf.insert(actual_byte, &self.content);

        let content_len = self.content.len();

        // Adjust all cursors - this needs to properly handle multi-line content
        shift_cursors(&mut buf.cursors, buf.primary_cursor, |start_byte, end_byte| {
            if start_byte > actual_byte {
                (start_byte + content_len, end_byte + content_len)
            } else if end_byte >= actual_byte {
                (start_byte, end_byte + content_len)
            } else {
                (start_byte, end_byte)
            }
        });

        let end = buf.get_edit_part(actual_byte + self.content.len());
        buf.register_input_edit(start, start, end);
        buf.version += 1;

        let inverse = Box::new(Delete {
            byte: self.byte,
            len: self.content.chars().count(),
        });

        ActionResult::new(true, inverse)
    }
}

/// Deletes text from a `TextBuffer`
pub struct Delete {
    pub byte: usize,
    /// Length in **chars** (not bytes) of text to delete
    pub len: usize,
}

impl BufferAction for Delete {
    fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.byte > buf.len() {
            return ActionResult::none(false);
        }

        let char_idx = buf.byte_to_char_clamped(self.byte);

        let del_start_byte = buf.char_to_byte_clamped(char_idx);

        let del_end_byte = if buf.len_chars() < char_idx + self.len {
            buf.char_to_byte_clamped(buf.len_chars())
        } else {
            buf.char_to_byte_clamped(char_idx + self.len)
        };

        let start = buf.get_edit_part(del_start_byte);
        let old_end = buf.get_edit_part(del_end_byte);

        if del_end_byte > buf.len() {
            return ActionResult::none(false);
        }

        if del_start_byte == del_end_byte {
            return ActionResult::none(false);
        }

        // Store the removed content for the inverse (Insert) action
        let removed = buf
            .slice_to_string(del_start_byte, del_end_byte)
            .unwrap_or_default();
        let bytes_removed = del_end_byte - del_start_byte;

        buf.remove_range(del_start_byte..del_end_byte);

        // Adjust other cursors to account for the deleted text
        shift_cursors(&mut buf.cursors, buf.primary_cursor, |start_byte, end_byte| {
            let new_start = if start_byte >= del_end_byte {
                start_byte.saturating_sub(bytes_removed)
            } else if start_byte >= del_start_byte {
                del_start_byte
            } else {
                start_byte
            };
            let new_end = if end_byte >= del_end_byte {
                end_byte.saturating_sub(bytes_removed)
            } else if end_byte >= del_start_byte {
                del_start_byte
            } else {
                end_byte
            };
            (new_start, new_end)
        });

        // Register the edit for syntax highlighting updates
        buf.register_input_edit(start, old_end, start);
        buf.version += 1;

        // The inverse of Delete is Insert
        let inverse = Box::new(Insert {
            byte: self.byte,
            content: removed,
        });

        ActionResult::new(true, inverse)
    }
}

/// An empty operation
pub struct NoOp;

impl BufferAction for NoOp {
    fn apply(&self, _buf: &mut TextBuffer) -> ActionResult {
        ActionResult::none(true)
    }
}
