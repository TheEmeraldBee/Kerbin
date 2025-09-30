use std::collections::BTreeMap;
use std::ops::Range;

use ascii_forge::window::crossterm::cursor::SetCursorStyle;

use crate::*;

#[derive(Default)]
pub struct BufferRenderer {
    extmarks: BTreeMap<u64, Extmark>,
    next_id: u64,

    pub lines: Vec<RenderLine>,

    /// Stores a byte position and cursor style for where the renderer
    /// should be rendering the cursor, allows for centeralized cursor rendering
    pub cursor: Option<(usize, SetCursorStyle)>,

    pub byte_scroll: usize,
    pub visual_scroll: usize,

    pub h_scroll: usize,
}

impl BufferRenderer {
    /// Creates a new extmark in this buffer, with a single byte pos
    ///
    /// # Arguments
    /// * `ns` - The Namespace of the Extmark
    /// * `byte` - The byte index to place the extmark.
    /// * `priority` - Rendering priority (higher → drawn on top).
    /// * `decorations` - A vector of [`ExtmarkDecoration`] items.
    ///
    /// # Returns
    /// The unique ID of the newly created extmark.
    pub fn add_extmark(
        &mut self,
        ns: impl ToString,
        byte: usize,
        priority: i32,
        decorations: Vec<ExtmarkDecoration>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let ext = Extmark {
            namespace: ns.to_string(),
            id,
            byte_range: byte..byte + 1,
            priority,
            decorations,
        };
        self.extmarks.insert(id, ext);
        id
    }

    /// Creates a new extmark in this buffer, taking up the given range of bytes
    ///
    /// # Arguments
    /// * `ns` - The Namespace of the Extmark
    /// * `byte_range` - The range of bytes that the decorations take up.
    /// * `priority` - Rendering priority (higher → drawn on top).
    /// * `decorations` - A vector of [`ExtmarkDecoration`] items.
    ///
    /// # Returns
    /// The unique ID of the newly created extmark.
    pub fn add_extmark_range(
        &mut self,
        ns: impl ToString,
        byte_range: Range<usize>,
        priority: i32,
        decorations: Vec<ExtmarkDecoration>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let ext = Extmark {
            namespace: ns.to_string(),
            id,
            byte_range,
            priority,
            decorations,
        };
        self.extmarks.insert(id, ext);
        id
    }

    /// Clears all extmarks with the given namespace from the system
    ///
    /// # Arguments
    /// * 'ns' - The namespace to remove
    pub fn clear_extmark_ns(&mut self, ns: impl AsRef<str>) {
        let ns = ns.as_ref();

        self.extmarks.retain(|_, e| e.namespace != ns);
    }

    /// Removes an extmark by its ID.
    ///
    /// # Arguments
    /// * `id` - The id of the extmark to remove
    ///
    /// # Returns
    /// `true` if successfully removed, `false` otherwise.
    pub fn remove_extmark(&mut self, id: u64) -> bool {
        self.extmarks.remove(&id).is_some()
    }

    /// Updates an existing extmark's decorations.
    ///
    /// # Arguments
    /// * `id` - The id of the extmark to update
    /// * `decorations` - A list of decorations to set the ID to
    ///
    /// # Returns
    /// `true` if the extmark exists and was updated, `false` otherwise.
    pub fn update_extmark(&mut self, id: u64, decorations: Vec<ExtmarkDecoration>) -> bool {
        if let Some(ext) = self.extmarks.get_mut(&id) {
            ext.decorations = decorations;
            true
        } else {
            false
        }
    }

    /// Queries extmarks intersecting a byte range.
    ///
    /// # Arguments
    /// * `range` - The byte range that should be included in the extmark list
    ///
    /// # Returns
    ///
    /// A list of extmarks found within the given range
    pub fn query_extmarks(&self, range: std::ops::Range<usize>) -> Vec<&Extmark> {
        let mut marks = self
            .extmarks
            .values()
            .filter(|ext| ext.byte_range.start < range.end && ext.byte_range.end >= range.start)
            .collect::<Vec<_>>();
        marks.sort_by(|x, y| x.priority.cmp(&y.priority));
        marks
    }
}
