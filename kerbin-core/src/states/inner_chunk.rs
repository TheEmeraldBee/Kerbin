use std::ops::{Deref, DerefMut};

use ratatui::{buffer::Buffer, layout::Rect};

use crate::CursorShape;

/// Internal chunk representing a ratatui buffer and an optional cursor
pub struct InnerChunk {
    buffer: Buffer,
    cursor: Option<(usize, u16, u16, CursorShape)>,
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
    /// Creates a new internal chunk from a ratatui Buffer
    pub fn new(buf: Buffer) -> Self {
        Self {
            buffer: buf,
            cursor: None,
        }
    }

    /// Returns the area of this chunk
    pub fn area(&self) -> Rect {
        self.buffer.area
    }

    /// Removes the cursor from this chunk
    pub fn remove_cursor(&mut self) {
        self.cursor = None;
    }

    /// Sets the cursor for this chunk with a specified priority, screen position, and shape
    pub fn set_cursor(&mut self, priority: usize, x: u16, y: u16, shape: CursorShape) {
        self.cursor = Some((priority, x, y, shape))
    }

    /// Returns whether a cursor is set for this chunk
    pub fn cursor_set(&self) -> bool {
        self.cursor.is_some()
    }

    /// Returns a reference to the full cursor information (priority, x, y, shape)
    pub fn get_cursor(&self) -> Option<&(usize, u16, u16, CursorShape)> {
        self.cursor.as_ref()
    }
}
