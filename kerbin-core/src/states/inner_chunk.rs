use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};

use crate::*;

pub type RenderFunc = Arc<Box<dyn Fn(&mut Window, Vec2) + Send + Sync>>;

/// Internal chunk representing a buffer and an optional cursor
pub struct InnerChunk {
    buffer: Buffer,
    pub render_items: Vec<(Vec2, RenderFunc)>,
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
    /// Creates a new internal chunk
    pub fn new(buf: Buffer) -> Self {
        Self {
            buffer: buf,
            render_items: vec![],
            cursor: None,
        }
    }

    /// Registers a special function that will render to the window
    pub fn register_item(&mut self, pos: impl Into<Vec2>, func: RenderFunc) {
        self.render_items.push((pos.into(), func));
    }

    /// Removes the cursor from this chunk
    pub fn remove_cursor(&mut self) {
        self.cursor = None;
    }

    /// Sets the cursor for this chunk with a specified priority and style
    pub fn set_cursor(&mut self, priority: usize, pos: Vec2, style: SetCursorStyle) {
        self.cursor = Some((priority, pos, style))
    }

    /// Returns the position of the cursor if set
    pub fn cursor_pos(&self) -> Option<Vec2> {
        self.cursor.as_ref().map(|x| x.1)
    }

    /// Returns whether a cursor is set for this chunk
    pub fn cursor_set(&self) -> bool {
        self.cursor.is_some()
    }

    /// Returns a reference to the full cursor information
    pub fn get_full_cursor(&self) -> &Option<(usize, Vec2, SetCursorStyle)> {
        &self.cursor
    }
}
