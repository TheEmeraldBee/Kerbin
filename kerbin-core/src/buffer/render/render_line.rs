use std::sync::Arc;

use crate::*;
use ascii_forge::prelude::*;
use unicode_width::*;

/// A pre-visual representation of what a rendered byte line looks like
/// This is updated so that visually we can find where bytes should be resulted
/// Allows for visual scrolling to correctly work
#[derive(Default)]
pub struct RenderLine {
    /// A list of elements and their visual column start positions, and the element that will be
    /// drawn, this should allow for easy systems to render elements to the screen
    elements: Vec<(usize, RenderLineElement)>,
}

impl RenderLine {
    /// Renders the Line to the passed buffer
    /// Will only render a max of 1 y location, and the buffer's width
    /// This will automatically apply scrolling algorithms to the system,
    /// Making rendering the line very easy
    ///
    /// # Arguments
    /// * `buffer`: The visual buffer to render to
    /// * `loc`: The offset on the buffer to render at
    /// * `horizontal_scroll`: The scroll of the line to apply
    pub fn render(&self, chunk: &mut InnerChunk, loc: Vec2, horizontal_scroll: usize) {
        let viewport_width = chunk.size().x.saturating_sub(loc.x) as usize;
        let mut render_col = 0_u16;

        for (elem_start_col, elem) in &self.elements {
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

/// A Renderable element for a RenderLine to render to the screen
pub enum RenderLineElement {
    /// A character from the byte line with a style applied
    RopeChar(char, usize, ContentStyle),

    /// A text element with a style (can be any text)
    Text(String, ContentStyle),

    /// An element that's rendering is called straight by window
    /// Should be paired with a ReservedSpace to correctly reserve space
    /// for the widget contained
    Element(Arc<Box<dyn Fn(&mut Window, Vec2) + Send + Sync>>),

    /// A reserved width in columns for inline element rendering (height of 1 only)
    ReservedSpace(usize),
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

    /// Takes the element and returns it's width in visual columns
    pub fn compute_size(&self) -> usize {
        match self {
            Self::RopeChar(ch, _, _) => ch.width().unwrap_or(1),
            Self::Text(t, _) => t.width(),
            Self::ReservedSpace(w) => *w,
            Self::Element(_) => 1,
        }
    }

    /// Render the element with clipping support for scrolling
    ///
    /// # Arguments
    /// * `buffer` - The buffer to render to
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
            Self::Element(func) => {
                chunk.register_item(pos, func.clone());
            }
            Self::ReservedSpace(w) => {
                let visible_width = if skip >= *w {
                    0
                } else {
                    (*w - skip).min(max_width)
                };

                if visible_width > 0 {
                    render!(chunk, pos => [" ".repeat(visible_width)]);
                }
            }
        }
    }
}
