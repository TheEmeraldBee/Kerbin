use std::{ops::Range, sync::Arc};

use ascii_forge::{
    math::Vec2,
    window::{crossterm::cursor::SetCursorStyle, Buffer, ContentStyle},
};

use crate::OverlayPositioning;

/// Determines how the extmark moves when text is edited
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ExtmarkGravity {
    /// When text is inserted at the extmark position, the mark moves right
    #[default]
    Right,
    /// When text is inserted at the extmark position, the mark stays in place
    Left,
}

/// Controls whether the extmark should adjust to text changes
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ExtmarkAdjustment {
    /// Extmark moves with text edits
    #[default]
    Track,

    /// Extmark stays at its original byte position regardless of edits
    Fixed,

    /// Extmark is deleted when text in its range is deleted
    DeleteOnDelete,
}

/// Types of decorations that can be attached to an [`Extmark`]
pub enum ExtmarkDecoration {
    /// Highlight a region of text with a named highlight group
    Highlight { hl: ContentStyle },

    /// Insert “virtual” text inline after buffer byte position
    VirtText {
        text: String,
        hl: Option<ContentStyle>,
    },

    /// Given the byte position of the decoration, render this element with an offset
    OverlayElement {
        offset: Vec2,
        elem: Arc<Buffer>,
        z_index: i32,
        clip_to_viewport: bool,
        positioning: OverlayPositioning,
    },

    /// Display a cursor (block/bar/underline), only one of these can exist on the state at a time
    Cursor { style: SetCursorStyle },

    /// Reserve the given lines after the buf
    FullElement { elem: Arc<Buffer> },
}

/// An anchored “mark” in a buffer, augmented with one or more decorations
pub struct Extmark {
    /// An identifier for the file version for which the extmark was registered
    pub file_version: u128,

    /// Unique identifier for programmatic reference and removal
    pub id: u64,

    /// Shared identifier marking relationships to other extmarks for removal
    pub namespace: String,

    pub byte_range: Range<usize>,

    /// Priority controls layer ordering
    pub priority: i32,

    /// List of one or more decorations applied at this mark
    pub decorations: Vec<ExtmarkDecoration>,

    pub gravity: ExtmarkGravity,
    pub adjustment: ExtmarkAdjustment,

    /// Whether the extmark should expand when item is inserted into range
    pub expand_on_insert: bool,
}

pub struct ExtmarkBuilder {
    namespace: String,
    byte_range: Range<usize>,

    priority: i32,

    decorations: Vec<ExtmarkDecoration>,

    gravity: ExtmarkGravity,
    adjustment: ExtmarkAdjustment,

    expand_on_insert: bool,
}

impl ExtmarkBuilder {
    pub fn new(ns: impl ToString, byte: usize) -> Self {
        Self {
            namespace: ns.to_string(),
            byte_range: byte..byte + 1,
            priority: 0,

            decorations: vec![],

            gravity: ExtmarkGravity::default(),
            adjustment: ExtmarkAdjustment::default(),

            expand_on_insert: false,
        }
    }

    pub fn new_range(ns: impl ToString, byte_range: Range<usize>) -> Self {
        Self {
            namespace: ns.to_string(),
            byte_range,
            priority: 0,

            decorations: vec![],

            gravity: ExtmarkGravity::default(),
            adjustment: ExtmarkAdjustment::default(),

            expand_on_insert: false,
        }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_decoration(mut self, decoration: ExtmarkDecoration) -> Self {
        self.decorations.push(decoration);
        self
    }

    pub fn with_decorations(
        mut self,
        decorations: impl IntoIterator<Item = ExtmarkDecoration>,
    ) -> Self {
        self.decorations.extend(decorations);
        self
    }

    pub fn with_gravity(mut self, gravity: ExtmarkGravity) -> Self {
        self.gravity = gravity;
        self
    }

    pub fn with_adjustment(mut self, adjustment: ExtmarkAdjustment) -> Self {
        self.adjustment = adjustment;
        self
    }

    pub fn with_expand_on_insert(mut self, expand: bool) -> Self {
        self.expand_on_insert = expand;
        self
    }

    pub fn build(self, id: u64, file_version: u128) -> Extmark {
        Extmark {
            id,
            file_version,
            namespace: self.namespace,
            byte_range: self.byte_range,
            priority: self.priority,

            decorations: self.decorations,

            adjustment: self.adjustment,
            gravity: self.gravity,

            expand_on_insert: self.expand_on_insert,
        }
    }
}
