use crate::RopeExts;

use super::TextBuffer;

/// A result of an action that stores whether the action was applied
/// and returns the inverse of the action for applying undo and redo.
pub struct ActionResult {
    /// `true` if the action was successfully applied, `false` otherwise.
    pub success: bool,
    /// A boxed `BufferAction` representing the inverse of the applied action.
    /// This is used for undo/redo functionality.
    pub action: Box<dyn BufferAction>,
}

impl ActionResult {
    /// Creates a new `ActionResult`.
    ///
    /// # Arguments
    ///
    /// * `success`: A boolean indicating if the action was successful.
    /// * `action`: A `Box<dyn BufferAction>` representing the inverse action.
    ///
    /// # Returns
    ///
    /// A new `ActionResult` instance.
    pub fn new(success: bool, action: Box<dyn BufferAction>) -> Self {
        Self { success, action }
    }

    /// Creates an `ActionResult` with a `NoOp` inverse action.
    ///
    /// This is useful for actions that don't have a direct inverse or
    /// for cases where only the success status matters for undo/redo.
    ///
    /// # Arguments
    ///
    /// * `success`: A boolean indicating if the action was successful.
    ///
    /// # Returns
    ///
    /// A new `ActionResult` with a `NoOp` inverse.
    pub fn none(success: bool) -> Self {
        Self::new(success, Box::new(NoOp))
    }
}

/// A system that treats changes to a `TextBuffer` as inversable actions.
///
/// This trait allows for a consistent system to handle modifications to the rope,
/// abstracting over many internal requirements of the editor engine, and
/// enabling robust undo/redo functionality.
pub trait BufferAction: Send + Sync {
    /// Applies the action to the given `TextBuffer`.
    ///
    /// This method performs the actual modification on the buffer. It should
    /// also return an `ActionResult` which includes a boolean indicating
    /// success and a boxed `BufferAction` representing the inverse operation.
    ///
    /// # Arguments
    ///
    /// * `buf`: A mutable reference to the `TextBuffer` to which the action should be applied.
    ///
    /// # Returns
    ///
    /// An `ActionResult` describing the outcome of the application and its inverse.
    fn apply(&self, buf: &mut TextBuffer) -> ActionResult;
}

/// An action that inserts text at a given byte offset in the `TextBuffer`.
///
/// Fails if the specified `byte` offset is beyond the current length of the rope.
pub struct Insert {
    /// The byte offset within the `TextBuffer` where the content should be inserted.
    /// Will be converted into a valid char boundary index
    pub byte: usize,

    /// The string content to be inserted at the specified location.
    pub content: String,
}

impl BufferAction for Insert {
    fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.byte > buf.rope.len() {
            return ActionResult::none(false);
        }

        let actual_byte = buf.rope.byte_to_char_boundary_byte(self.byte);
        let start = buf.get_edit_part(actual_byte);

        // This already handles newlines correctly
        buf.rope.insert(actual_byte, &self.content);

        let content_len = self.content.len();

        // Adjust all cursors - this needs to properly handle multi-line content
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

        let end = buf.get_edit_part(actual_byte + self.content.len());
        buf.register_input_edit(start, start, end);

        let inverse = Box::new(Delete {
            byte: self.byte,
            len: self.content.len(),
        });

        ActionResult::new(true, inverse)
    }
}

/// An action to delete text from a `TextBuffer`.
///
/// Fails if the specified range to delete (`byte` to `byte + len`)
/// extends beyond the end of the rope.
pub struct Delete {
    /// The starting byte offset of the text to be deleted.
    /// Will be turned into a char index internally
    pub byte: usize,

    /// The length in **chars** of the text to be deleted.
    pub len: usize,
}

impl BufferAction for Delete {
    fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if self.byte > buf.rope.len() {
            return ActionResult::none(false);
        }

        let char_idx = buf.rope.byte_to_char_idx(self.byte);

        let start_byte = buf.rope.char_to_byte_idx(char_idx);

        let end_byte = if buf.rope.len_chars() < char_idx + self.len {
            buf.rope.char_to_byte_idx(buf.rope.len())
        } else {
            buf.rope.char_to_byte_idx(char_idx + self.len)
        };

        let start = buf.get_edit_part(start_byte);
        let old_end = buf.get_edit_part(end_byte);

        if end_byte > buf.rope.len() {
            return ActionResult::none(false);
        }

        if start_byte == end_byte {
            return ActionResult::none(false);
        }

        // Store the removed content for the inverse (Insert) action
        let removed = buf.rope.slice(start_byte..end_byte).to_string();

        buf.rope.remove(start_byte..end_byte);

        // Adjust other cursors to account for the deleted text
        for (i, cursor) in buf.cursors.iter_mut().enumerate() {
            if i == buf.primary_cursor {
                continue; // Skip primary cursor
            }

            let start_byte = *cursor.sel.start();
            let end_byte = *cursor.sel.end();
            let mut new_start = start_byte;
            let mut new_end = end_byte;

            // If a cursor's selection is entirely after the deleted region, shift it left
            if start_byte >= self.byte + self.len {
                new_start = start_byte.saturating_sub(self.len);
            } else if start_byte >= self.byte {
                // If a cursor's selection starts within or before the deleted region
                // and extends beyond, collapse its start to the deletion point
                new_start = self.byte;
            }

            // Similarly for the end of the selection
            if end_byte >= self.byte + self.len {
                new_end = end_byte.saturating_sub(self.len);
            } else if end_byte >= self.byte {
                new_end = self.byte;
            }

            cursor.sel = new_start..=new_end;
        }

        // Register the edit for syntax highlighting updates
        buf.register_input_edit(start, old_end, start);

        // The inverse of Delete is Insert
        let inverse = Box::new(Insert {
            byte: self.byte,
            content: removed,
        });

        ActionResult::new(true, inverse)
    }
}

/// An empty operation.
///
/// This action does nothing and always succeeds. It is useful for scenarios
/// where an action cannot or should not be undone (e.g., when an action
/// changes state in a non-reversible way that the action system can't track).
pub struct NoOp;

impl BufferAction for NoOp {
    fn apply(&self, _buf: &mut TextBuffer) -> ActionResult {
        ActionResult::none(true)
    }
}
