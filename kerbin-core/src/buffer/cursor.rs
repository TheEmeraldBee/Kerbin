use std::ops::RangeInclusive;

/// A selected point of text within a `TextBuffer`.
///
/// This struct is the core for editing files, marking a selection, and is
/// essential for supporting multicursor functionality.
#[derive(Clone, Debug)]
pub struct Cursor {
    /// Indicates whether the "caret" (active end of the selection) is at the start (`true`)
    /// or end (`false`) of the `sel` range. This affects how selections are extended.
    pub(crate) at_start: bool,
    /// The inclusive byte range of the text selected by this cursor.
    pub(crate) sel: RangeInclusive<usize>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            at_start: false,
            sel: 0..=0, // Default to a collapsed selection at byte 0
        }
    }
}

impl Cursor {
    /// Returns the byte position of where the actual cursor (caret) would be.
    ///
    /// This is either the start or the end of the selection, depending on `at_start`.
    ///
    /// # Returns
    ///
    /// The byte index of the cursor's caret.
    pub fn get_cursor_byte(&self) -> usize {
        match self.at_start {
            true => *self.sel.start(),
            false => *self.sel.end(),
        }
    }

    /// Returns `true` if the cursor's caret is at the start of its selection, `false` otherwise.
    pub fn at_start(&self) -> bool {
        self.at_start
    }

    /// Sets whether the cursor's caret should be at the start or end of its selection.
    ///
    /// # Arguments
    ///
    /// * `at_start`: `true` to place the caret at the start, `false` for the end.
    pub fn set_at_start(&mut self, at_start: bool) {
        self.at_start = at_start
    }

    /// Returns a reference to the inclusive byte range of the selection for this cursor.
    ///
    /// # Returns
    ///
    /// A `&RangeInclusive<usize>` representing the selection.
    pub fn sel(&self) -> &RangeInclusive<usize> {
        &self.sel
    }

    /// Sets the inclusive byte range of the selection for this cursor.
    ///
    /// # Arguments
    ///
    /// * `range`: The new `RangeInclusive<usize>` for the selection.
    pub fn set_sel(&mut self, range: RangeInclusive<usize>) {
        self.sel = range;
    }

    /// Collapses the selection into the location of the cursor's caret.
    ///
    /// If `at_start` is true, the selection collapses to `*sel.start()`.
    /// If `at_start` is false, it collapses to `*sel.end()`.
    pub fn collapse_sel(&mut self) {
        match self.at_start {
            true => self.sel = *self.sel.start()..=*self.sel.start(),
            false => self.sel = *self.sel.end()..=*self.sel.end(),
        }
    }
}
