use std::{ops::Range, sync::Arc};

use ratatui::{buffer::Buffer as RatatuiBuffer, style::Style};

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

/// The shape of a terminal cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    BlinkingBlock,
    Bar,
    BlinkingBar,
    UnderScore,
    BlinkingUnderScore,
}

impl CursorShape {
    pub fn to_crossterm_style(self) -> crossterm::cursor::SetCursorStyle {
        use crossterm::cursor::SetCursorStyle;
        match self {
            CursorShape::Block => SetCursorStyle::SteadyBlock,
            CursorShape::BlinkingBlock => SetCursorStyle::BlinkingBlock,
            CursorShape::Bar => SetCursorStyle::SteadyBar,
            CursorShape::BlinkingBar => SetCursorStyle::BlinkingBar,
            CursorShape::UnderScore => SetCursorStyle::SteadyUnderScore,
            CursorShape::BlinkingUnderScore => SetCursorStyle::BlinkingUnderScore,
        }
    }
}

/// A chunk of styled text used for virtual text display.
#[derive(Clone, Debug)]
pub struct StyledChunk {
    pub text: String,
    pub style: Style,
}

/// Where virtual text is rendered relative to its anchor position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtTextPos {
    /// After the end of the line
    Eol,
    /// Overlay on top of existing text at the position
    Overlay,
    /// Inserted inline, shifting subsequent content
    Inline,
    /// Right-aligned in the viewport
    RightAlign,
}

/// The kind of decoration applied to an extmark.
#[derive(Clone, Debug)]
pub enum ExtmarkKind {
    /// A cursor mark at a single byte position
    Cursor { style: Style, shape: CursorShape },

    /// A highlight over the byte range encoded in `Extmark::byte_range`
    Highlight { style: Style },

    /// Virtual text anchored to `byte_range.start`
    VirtualText {
        chunks: Vec<StyledChunk>,
        pos: VirtTextPos,
    },

    /// Conceal the byte range — hide or replace text visually
    Conceal {
        replacement: Option<String>,
        style: Option<Style>,
    },

    /// A floating popup anchored to `byte_range.start`.
    ///
    /// `offset_x`/`offset_y` are character offsets from the anchor's screen position.
    /// The popup is clipped to the viewport.
    Overlay {
        content: Arc<RatatuiBuffer>,
        offset_x: i32,
        offset_y: i32,
        z_index: i32,
    },
}

/// An anchored "mark" in a buffer, augmented with a decoration kind
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

    /// The decoration kind applied at this mark
    pub kind: ExtmarkKind,

    pub gravity: ExtmarkGravity,
    pub adjustment: ExtmarkAdjustment,

    /// Whether the extmark should expand when item is inserted into range
    pub expand_on_insert: bool,
}

pub struct ExtmarkBuilder {
    namespace: String,
    byte_range: Range<usize>,

    priority: i32,

    kind: Option<ExtmarkKind>,

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

            kind: None,

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

            kind: None,

            gravity: ExtmarkGravity::default(),
            adjustment: ExtmarkAdjustment::default(),

            expand_on_insert: false,
        }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_kind(mut self, kind: ExtmarkKind) -> Self {
        self.kind = Some(kind);
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

            kind: self.kind.unwrap_or(ExtmarkKind::Highlight {
                style: Style::default(),
            }),

            adjustment: self.adjustment,
            gravity: self.gravity,

            expand_on_insert: self.expand_on_insert,
        }
    }
}
