use std::sync::Arc;

use crate::*;
use ascii_forge::prelude::*;
use unicode_width::*;

/// Information about an overlay that needs to be rendered
#[derive(Clone)]
pub struct OverlayInfo {
    /// The line index this overlay is anchored to
    pub anchor_line: usize,
    /// The column within that line (in visual columns)
    pub anchor_col: usize,
    /// The byte position this overlay is anchored to
    pub anchor_byte: usize,
    /// Offset from the anchor position
    pub offset: Vec2,
    /// The element to render
    pub elem: Arc<Buffer>,
    /// Z-index for layering (higher renders on top)
    pub z_index: i32,
    /// Whether the overlay should be clipped to viewport
    pub clip_to_viewport: bool,
    /// Positioning mode
    pub positioning: OverlayPositioning,
}

/// A pre-visual representation of what a rendered byte line looks like
/// This is updated so that visually we can find where bytes should be resulted
/// Allows for visual scrolling to correctly work
pub struct RenderLine {
    /// A list of elements and their visual column start positions, and the element that will be
    /// drawn, this should allow for easy systems to render elements to the screen
    elements: Vec<(usize, RenderLineElement)>,

    /// The far-left piece of the rendered line
    /// Rendered at the BufferGutter location
    gutter: Buffer,
}

impl Default for RenderLine {
    fn default() -> Self {
        Self {
            elements: vec![],
            gutter: Buffer::new((6, 1)),
        }
    }
}

/// Multi-line overlay renderer
#[derive(Default)]
pub struct OverlayRenderer {
    overlays: Vec<OverlayInfo>,
}

impl OverlayRenderer {
    /// Collect overlays from all rendered lines
    pub fn collect_from_lines(&mut self, lines: &[RenderLine], start_line: usize) {
        self.overlays.clear();

        for (line_offset, line) in lines.iter().enumerate() {
            let line_idx = start_line + line_offset;
            let mut line_overlays = line.extract_overlays();

            // Set the correct line index for each overlay
            for (_, overlay) in &mut line_overlays {
                overlay.anchor_line = line_idx;
            }

            self.overlays
                .extend(line_overlays.into_iter().map(|(_, o)| o));
        }

        // Sort by z-index (lower first, higher on top)
        self.overlays.sort_by_key(|o| o.z_index);
    }

    /// Render all overlays for the visible lines
    pub fn render_all(
        &self,
        chunk: &mut InnerChunk,
        base_loc: Vec2,
        start_line: usize,
        horizontal_scroll: usize,
        line_height: u16,
    ) {
        let viewport_width = chunk.size().x.saturating_sub(base_loc.x) as usize;
        let viewport_height = chunk.size().y.saturating_sub(base_loc.y) as usize;

        for overlay in &self.overlays {
            // Calculate the base position of the anchor
            let line_offset = overlay.anchor_line.saturating_sub(start_line);
            let line_y = base_loc.y + (line_offset as u16 * line_height);

            // Calculate horizontal position based on positioning mode
            let base_x = match overlay.positioning {
                OverlayPositioning::RelativeToChar => {
                    // Position relative to the character, affected by scroll
                    if overlay.anchor_col < horizontal_scroll {
                        continue; // Anchor is scrolled off-screen
                    }
                    base_loc.x + (overlay.anchor_col.saturating_sub(horizontal_scroll) as u16)
                }
                OverlayPositioning::RelativeToLine => {
                    // Position relative to line start, affected by scroll
                    if overlay.anchor_col < horizontal_scroll {
                        continue; // Would be off-screen
                    }
                    base_loc.x + (overlay.anchor_col.saturating_sub(horizontal_scroll) as u16)
                }
                OverlayPositioning::ViewportFixed => {
                    // Fixed position in viewport, ignoring scroll
                    base_loc.x + overlay.anchor_col as u16
                }
            };

            let base_pos = vec2(base_x, line_y);
            let final_pos = base_pos + overlay.offset;

            // Check if overlay is within viewport bounds
            if overlay.clip_to_viewport {
                let overlay_size = overlay.elem.size();

                // Skip if completely outside viewport
                if final_pos.x >= base_loc.x + viewport_width as u16
                    || final_pos.y >= base_loc.y + viewport_height as u16
                    || final_pos.x + overlay_size.x <= base_loc.x
                    || final_pos.y + overlay_size.y <= base_loc.y
                {
                    continue;
                }
            }

            // Render the overlay
            render!(chunk, final_pos => [&overlay.elem]);
        }
    }
}

impl RenderLine {
    /// Returns a mutable reference to the buffer of the line's gutter
    pub fn gutter_mut(&mut self) -> &mut Buffer {
        &mut self.gutter
    }

    /// Returns the buffer of the line's gutter
    pub fn gutter(&self) -> &Buffer {
        &self.gutter
    }

    /// Extract overlay information from this line
    /// Returns overlays with their column positions
    pub fn extract_overlays(&self) -> Vec<(usize, OverlayInfo)> {
        let mut overlays = vec![];

        for (col, elem) in &self.elements {
            if let RenderLineElement::OverlayElement {
                anchor_byte,
                offset,
                elem: buffer,
                z_index,
                clip_to_viewport,
                positioning,
            } = elem
            {
                overlays.push((
                    *col,
                    OverlayInfo {
                        anchor_line: 0, // Will be set by the caller
                        anchor_col: *col,
                        anchor_byte: *anchor_byte,
                        offset: *offset,
                        elem: buffer.clone(),
                        z_index: *z_index,
                        clip_to_viewport: *clip_to_viewport,
                        positioning: *positioning,
                    },
                ));
            }
        }

        overlays
    }

    /// Renders the gutter to the location
    ///
    /// # Arguments
    /// `chunk`: The visual chunk to render to
    /// `loc`: The position to render at
    pub fn render_gutter(&self, chunk: &mut InnerChunk, loc: Vec2) {
        render!(chunk, loc => [ &self.gutter ]);
    }

    /// Renders the Line to the passed buffer Will only render a max of 1 y location, and the buffer's width
    /// This will automatically apply scrolling algorithms to the system,
    /// Making rendering the line very easy
    ///
    /// # Arguments
    /// * `chunk`: The visual chunk to render to
    /// * `loc`: The offset on the buffer to render at
    /// * `horizontal_scroll`: The scroll of the line to apply
    pub fn render(&self, chunk: &mut InnerChunk, loc: Vec2, horizontal_scroll: usize) {
        let viewport_width = chunk.size().x.saturating_sub(loc.x) as usize;
        let mut render_col = 0_u16;

        for (elem_start_col, elem) in &self.elements {
            // Skip overlay elements in base rendering
            if elem.is_overlay() {
                continue;
            }

            let elem_width = elem.compute_size();
            let elem_end_col = elem_start_col + elem_width;

            // Skip elements entirely before the scroll position
            if elem_end_col <= horizontal_scroll {
                continue;
            }

            // Stop if we're past the viewport
            if *elem_start_col >= horizontal_scroll + viewport_width {
                break;
            }

            // Calculate how much of this element is visible
            let visible_start = elem_start_col.max(&horizontal_scroll) - horizontal_scroll;
            let visible_end =
                elem_end_col.min(horizontal_scroll + viewport_width) - horizontal_scroll;

            // Skip offset within the element
            let skip_in_element = if *elem_start_col < horizontal_scroll {
                horizontal_scroll - elem_start_col
            } else {
                0
            };

            let visible_width = visible_end.saturating_sub(visible_start);

            if visible_width > 0 {
                elem.render(
                    chunk,
                    loc + vec2(render_col, 0),
                    skip_in_element,
                    visible_width,
                );

                render_col += visible_width as u16;
            }
        }
    }

    /// Calculates the size of the last element, returning the total length of the Line
    ///
    /// # Returns
    /// The width, in visual columns of the RenderLine
    pub fn calculate_size(&self) -> usize {
        self.elements
            .last()
            .map(|x| x.0 + x.1.compute_size())
            .unwrap_or(0)
    }

    /// Adds an element to the line, taking ownership for a builder pattern
    ///
    /// # Arguments
    /// * `element`: the element to add into the system
    ///
    /// # Returns
    /// This line with a new element in it, if you want a non-ownership function, look at `RenderLine::element`
    pub fn with_element(mut self, element: RenderLineElement) -> Self {
        self.elements.push((
            self.elements
                .last()
                .map(|x| x.0 + x.1.compute_size())
                .unwrap_or(0),
            element,
        ));
        self
    }

    /// Adds an element to the line, taking ownership for a builder pattern
    ///
    /// # Arguments
    /// * `element`: the element to add into the system
    ///
    /// # Returns
    /// This line with a new element in it, if you want a version with ownership, look at
    /// `RenderLine::with_element`
    pub fn element(&mut self, element: RenderLineElement) -> &mut Self {
        self.elements.push((
            self.elements
                .last()
                .map(|x| x.0 + x.1.compute_size())
                .unwrap_or(0),
            element,
        ));
        self
    }

    /// Search the line for  a byte within it
    ///
    /// # Arguments
    /// * `byte`: the byte to search for
    ///
    /// # Returns
    /// An optional column that the rope byte is being rendered at
    pub fn byte_to_col(&self, byte: usize) -> Option<usize> {
        for (col, elem) in &self.elements {
            if elem.is_rope_byte(byte) {
                return Some(*col);
            }
        }
        None
    }
}

/// Controls how an overlay is positioned
#[derive(Clone, Copy, Debug)]
pub enum OverlayPositioning {
    /// Relative to the character/byte position (affected by scroll)
    RelativeToChar,
    /// Relative to the line start (moves with horizontal scroll)
    RelativeToLine,
    /// Fixed to viewport (ignores scroll completely)
    ViewportFixed,
}

/// A Renderable element for a RenderLine to render to the screen
pub enum RenderLineElement {
    /// A character from the byte line with a style applied
    RopeChar(char, usize, ContentStyle),

    /// A text element with a style (can be any text)
    Text(String, ContentStyle),

    /// An element that's rendering is called straight by window
    /// Should be paired with ReservedSpace to correctly reserve space
    /// for the widget contained
    Element(Arc<Buffer>),

    /// A reserved width in columns for inline element rendering (height of 1 only)
    ReservedSpace(usize),

    /// An overlay element rendered at an offset from the byte position
    /// Does not take up space in the line layout
    /// Enhanced overlay element with positioning options
    OverlayElement {
        /// The byte position this overlay is anchored to
        anchor_byte: usize,
        /// Offset from the anchor position
        offset: Vec2,
        /// The element to render
        elem: Arc<Buffer>,
        /// Z-index for layering (higher renders on top)
        z_index: i32,
        /// Whether the overlay should be clipped to viewport
        clip_to_viewport: bool,
        /// Positioning mode
        positioning: OverlayPositioning,
    },
}

impl RenderLineElement {
    /// Returns whether the byte passed is within the
    /// passed character for the inner TextBuffer rope
    pub fn is_rope_byte(&self, byte: usize) -> bool {
        match self {
            Self::RopeChar(_, b, _) => *b == byte,
            _ => false,
        }
    }

    pub fn is_overlay(&self) -> bool {
        match self {
            Self::RopeChar(_, _, _) => false,
            Self::Text(_, _) => false,
            Self::ReservedSpace(_) => false,
            Self::Element(_) => false,
            Self::OverlayElement { .. } => true,
        }
    }

    /// Takes the element and returns it's width in visual columns
    pub fn compute_size(&self) -> usize {
        match self {
            Self::RopeChar(ch, _, _) => ch.width().unwrap_or(1),
            Self::Text(t, _) => t.width(),
            Self::ReservedSpace(w) => *w,
            Self::Element(w) => w.size().x as usize,
            Self::OverlayElement { .. } => 0, // Takes no space in layout
        }
    }

    pub fn overlay_z_index(&self) -> i32 {
        match self {
            Self::OverlayElement { z_index, .. } => *z_index,
            _ => 0,
        }
    }

    /// Render the element with clipping support for scrolling
    ///
    /// # Arguments
    /// * `chunk` - The chunk to render to
    /// * `pos` - The position to render at
    /// * `skip` - How many visual columns to skip from the start
    /// * `max_width` - Maximum width to render
    pub fn render(&self, chunk: &mut InnerChunk, pos: Vec2, skip: usize, max_width: usize) {
        match self {
            Self::RopeChar(ch, _, st) => {
                let char_width = ch.width().unwrap_or(1);

                // If skip is within this char, we can't render it (would show partial char)
                if skip > 0 && skip < char_width {
                    return;
                }

                // If we're past the skip, render the char
                if skip == 0 && char_width <= max_width {
                    render!(chunk, pos => [ st.apply(ch) ]);
                }
            }
            Self::Text(txt, st) => {
                // Calculate which part of the text to show
                let chars: Vec<char> = txt.chars().collect();
                let mut current_col = 0;
                let mut visible_chars = String::new();

                for ch in chars {
                    let ch_width = ch.width().unwrap_or(1);
                    let ch_start = current_col;
                    let ch_end = current_col + ch_width;

                    // Skip chars before the skip point
                    if ch_end <= skip {
                        current_col += ch_width;
                        continue;
                    }

                    // Stop if we've reached max_width
                    if ch_start >= skip + max_width {
                        break;
                    }

                    // Partial char at start - skip it
                    if ch_start < skip && ch_end > skip {
                        current_col += ch_width;
                        continue;
                    }

                    visible_chars.push(ch);
                    current_col += ch_width;
                }

                if !visible_chars.is_empty() {
                    render!(chunk, pos => [ st.apply(&visible_chars) ]);
                }
            }
            Self::Element(buf) => {
                render!(chunk, pos => [buf]);
            }
            Self::OverlayElement { .. } => {
                unimplemented!("Overlays should be handled by RenderLine directly")
            }
            Self::ReservedSpace(_) => {}
        }
    }
}
