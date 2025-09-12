use super::TextBuffer;

/// A result of an action that stores whether the action was applied
/// and returns the inverse of the action for applying undo and redo
pub struct ActionResult {
    pub success: bool,
    pub action: Box<dyn BufferAction>,
}

impl ActionResult {
    /// Creates a new BufferAction
    pub fn new(success: bool, action: Box<dyn BufferAction>) -> Self {
        Self { success, action }
    }

    /// Creates a NoOp action as a reverse, just requiring the success
    pub fn none(success: bool) -> Self {
        Self::new(success, Box::new(NoOp))
    }
}

/// System that treats changes to a TextBuffer like inversable actions
/// Allows for a consistent system to handle changes to the rope,
/// Abstracting over many internal requirements of the engine
pub trait BufferAction: Send + Sync {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult;
}

/// An action that inserts text at the given byte
/// Fails if the byte is after the end of the rope
pub struct Insert {
    /// The location of the start of the insert
    pub byte: usize,

    /// The content that should be added at the location
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

/// Action to delete text with a position and length of the engine
pub struct Delete {
    /// Byte location of the edit
    pub byte: usize,

    /// Length in **bytes** of the change
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

/// An empty operation,
/// Does nothing, always succeeds
/// useful for systems that can't be undone (can cause many issues)
pub struct NoOp;
impl BufferAction for NoOp {
    extern "C" fn apply(&self, _buf: &mut TextBuffer) -> ActionResult {
        ActionResult::none(true)
    }
}
