use std::ops::RangeInclusive;

/// A selected point of text within a `TextBuffer`
#[derive(Clone, Debug)]
pub struct Cursor {
    pub(crate) at_start: bool,
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
    /// Returns the byte position of where the actual cursor (caret) would be
    pub fn get_cursor_byte(&self) -> usize {
        match self.at_start {
            true => *self.sel.start(),
            false => *self.sel.end(),
        }
    }

    /// Returns `true` if the cursor's caret is at the start of its selection, `false` otherwise
    pub fn at_start(&self) -> bool {
        self.at_start
    }

    /// Sets whether the cursor's caret should be at the start or end of its selection
    pub fn set_at_start(&mut self, at_start: bool) {
        self.at_start = at_start
    }

    /// Returns a reference to the inclusive byte range of the selection for this cursor
    pub fn sel(&self) -> &RangeInclusive<usize> {
        &self.sel
    }

    /// Sets the inclusive byte range of the selection for this cursor
    pub fn set_sel(&mut self, range: RangeInclusive<usize>) {
        self.sel = range;
    }

    /// Collapses the selection into the location of the cursor's caret
    pub fn collapse_sel(&mut self) {
        match self.at_start {
            true => self.sel = *self.sel.start()..=*self.sel.start(),
            false => self.sel = *self.sel.end()..=*self.sel.end(),
        }
    }
}
