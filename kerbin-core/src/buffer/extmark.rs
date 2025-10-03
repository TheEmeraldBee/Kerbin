use std::ops::Range;

use ascii_forge::window::{ContentStyle, crossterm::cursor::SetCursorStyle};

use crate::RenderFunc;

/// Types of decorations that can be attached to an [`Extmark`].
///
/// Extmarks let you attach visual overlays to a [`TextBuffer`] without
/// modifying the actual text content. Multiple decorations can be combined.
pub enum ExtmarkDecoration {
    /// Highlight a region of text with a named highlight group.
    /// Takes an array and treats items as a fallback list
    Highlight { hl: ContentStyle },

    /// Insert “virtual” text inline before buffer position.
    VirtText {
        text: String,
        hl: Option<ContentStyle>,
    },

    /// Display a cursor (block/bar/underline), only one of these can exist on the state at a time,
    /// If you want your own, and to ignore the other, set a higher priority
    ///
    /// # Styles - Valid values of the style
    /// * `underscore`
    /// * `block`
    /// * `bar`
    /// * else: `block`
    Cursor { style: SetCursorStyle },

    /// Reserve the given lines after the buf
    /// Used for rendering complex states
    FullElement { height: u16, func: RenderFunc },
}

/// An anchored “mark” in a buffer, augmented with one or more decorations.
///
/// Extmarks are automatically shifted when text is inserted or deleted,
/// and can be queried during rendering.
pub struct Extmark {
    /// Unique identifier for programmatic reference and removal.
    pub id: u64,

    /// Shared identifier marking relationships to other extmarks for removal.
    pub namespace: String,

    pub byte_range: Range<usize>,

    /// Priority controls layer ordering. Higher = drawn later (on top).
    pub priority: i32,

    /// List of one or more decorations applied at this mark.
    pub decorations: Vec<ExtmarkDecoration>,
}
