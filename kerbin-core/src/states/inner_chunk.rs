use std::ops::{Deref, DerefMut};

use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};

use crate::*;

/// An internal chunk representing a buffer and an optional cursor.
///
/// This struct is used by the `Chunks` state to manage individual drawing areas
/// within the editor, each potentially having its own cursor.
pub struct InnerChunk {
    buffer: Buffer,
    cursor: Option<(usize, Vec2, SetCursorStyle)>,
}

impl Deref for InnerChunk {
    type Target = Buffer;
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for InnerChunk {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl InnerChunk {
    /// Creates a new `InnerChunk` with the given buffer and no cursor initially.
    ///
    /// # Arguments
    ///
    /// * `buf`: The `Buffer` to wrap.
    pub fn new(buf: Buffer) -> Self {
        Self {
            buffer: buf,
            cursor: None,
        }
    }

    /// Removes the cursor from this chunk, if one was set.
    pub fn remove_cursor(&mut self) {
        self.cursor = None;
    }

    /// Sets the cursor for this chunk with a specified priority, position, and style.
    ///
    /// The priority can be used to resolve conflicts if multiple chunks attempt to
    /// set the cursor simultaneously.
    ///
    /// # Arguments
    ///
    /// * `priority`: The priority level of this cursor. Higher values typically mean higher priority.
    /// * `pos`: The `Vec2` coordinates of the cursor within the chunk's buffer.
    /// * `style`: The `SetCursorStyle` to apply to the cursor.
    pub fn set_cursor(&mut self, priority: usize, pos: Vec2, style: SetCursorStyle) {
        self.cursor = Some((priority, pos, style))
    }

    /// Returns the position of the cursor if set.
    ///
    /// # Returns
    ///
    /// An `Option<Vec2>` representing the cursor's position within the chunk's buffer,
    /// or `None` if no cursor is set.
    pub fn cursor_pos(&self) -> Option<Vec2> {
        self.cursor.as_ref().map(|x| x.1)
    }

    /// Returns `true` if a cursor is set for this chunk, `false` otherwise.
    pub fn cursor_set(&self) -> bool {
        self.cursor.is_some()
    }

    /// Returns a reference to the full cursor information.
    ///
    /// This includes priority, position, and style, if a cursor is set.
    ///
    /// # Returns
    ///
    /// A reference to `Option<(usize, Vec2, SetCursorStyle)>`.
    pub fn get_full_cursor(&self) -> &Option<(usize, Vec2, SetCursorStyle)> {
        &self.cursor
    }
}
