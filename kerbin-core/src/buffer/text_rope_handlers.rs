use ropey::{RopeSlice, iter::Lines};

use crate::TextBuffer;
use crate::rope_exts::RopeExts;

/// Safe byte/char/line access for [`TextBuffer`].
///
/// Each operation exists in two forms:
/// - `foo(idx) -> Option<T>` — returns `None` for out-of-range indices (validation paths).
/// - `foo_clamped(idx) -> T` — clamps to valid range, always returns a value (rendering paths).
///
/// The 16×2 method surface is intentional: callers must explicitly choose
/// between fallible and clamped access. A macro would obscure this distinction.
pub trait SafeRopeAccess {
    /// Safely gets the line index for a byte index
    fn byte_to_line(&self, byte: usize) -> Option<usize>;

    /// Gets the line index for a byte index, clamping to the valid range
    fn byte_to_line_clamped(&self, byte: usize) -> usize;

    /// Safely gets the byte index for a line index
    fn line_to_byte(&self, line: usize) -> Option<usize>;

    /// Gets the byte index for a line index, clamping to the valid range
    fn line_to_byte_clamped(&self, line: usize) -> usize;

    /// Safely gets the char index for a byte index
    fn byte_to_char(&self, byte: usize) -> Option<usize>;

    /// Gets the char index for a byte index, clamping to the valid range
    fn byte_to_char_clamped(&self, byte: usize) -> usize;

    /// Safely gets the byte index for a char index
    fn char_to_byte(&self, char_idx: usize) -> Option<usize>;

    /// Gets the byte index for a char index, clamping to the valid range
    fn char_to_byte_clamped(&self, char_idx: usize) -> usize;

    /// Safely gets a character at the specified index
    fn char(&self, char_idx: usize) -> Option<char>;

    /// Gets a character at the specified index, clamping to the valid range
    fn char_clamped(&self, char_idx: usize) -> char;

    /// Safely gets a line slice
    fn line(&self, line_idx: usize) -> Option<RopeSlice<'_>>;

    /// Gets a line slice, clamping to the last line
    fn line_clamped(&self, line_idx: usize) -> RopeSlice<'_>;

    /// Safely gets an iterator over lines starting at byte
    fn lines_at(&self, byte: usize) -> Option<Lines<'_>>;

    /// Gets an iterator over lines starting at byte, clamping byte to len
    fn lines_at_clamped(&self, byte: usize) -> Lines<'_>;

    /// Safely gets the chunk at the specified byte
    fn chunk_at(&self, byte: usize) -> Option<(&str, usize, usize, usize)>;

    /// Gets the byte offset of the char boundary at or before the given byte, clamping input
    fn byte_to_char_boundary(&self, byte: usize) -> usize;

    /// Safely slices the rope
    fn slice_to_string(&self, start: usize, end: usize) -> Option<String>;

    /// Safely slices the rope, returning a RopeSlice
    fn slice(&self, start: usize, end: usize) -> Option<RopeSlice<'_>>;

    /// Safely slices the rope, returning a RopeSlice
    fn slice_clamped(&self, start: usize, end: usize) -> RopeSlice<'_>;

    /// Returns the entire buffer content as a String
    fn to_string(&self) -> String;

    /// Returns the length of the buffer in bytes
    fn len(&self) -> usize;

    /// Returns whether the buffer is empty or not
    fn is_empty(&self) -> bool;

    /// Returns the length of the buffer in chars
    fn len_chars(&self) -> usize;

    /// Returns the length of the buffer in lines
    fn len_lines(&self) -> usize;
}

impl TextBuffer {
    fn byte_to_line_inner(&self, byte: usize) -> usize {
        self.rope.byte_to_line(byte.min(self.rope.len_bytes()))
    }

    fn line_to_byte_inner(&self, line: usize) -> usize {
        self.rope.char_to_byte(self.rope.line_to_char(line))
    }

    fn byte_to_char_inner(&self, byte: usize) -> usize {
        self.rope.byte_to_char(byte.min(self.rope.len_bytes()))
    }

    fn char_to_byte_inner(&self, char_idx: usize) -> usize {
        self.rope.char_to_byte(char_idx.min(self.rope.len_chars()))
    }

    fn char_inner(&self, char_idx: usize) -> char {
        self.rope.char(char_idx)
    }

    fn line_inner(&self, line_idx: usize) -> RopeSlice<'_> {
        self.rope.line(line_idx.min(self.rope.len_lines().saturating_sub(1)))
    }

    fn lines_at_inner(&self, byte: usize) -> Lines<'_> {
        let byte = byte.min(self.rope.len_bytes());
        self.rope.lines_at(self.rope.byte_to_char(byte))
    }

    fn slice_bounds_valid(&self, start: usize, end: usize) -> bool {
        let len = self.rope.len_bytes();
        start <= len && end <= len && start <= end
    }

    fn slice_inner(&self, start: usize, end: usize) -> RopeSlice<'_> {
        self.rope
            .slice(self.rope.byte_to_char(start)..self.rope.byte_to_char(end))
    }
}

impl SafeRopeAccess for TextBuffer {
    fn byte_to_line(&self, byte: usize) -> Option<usize> {
        (byte <= self.rope.len_bytes()).then(|| self.byte_to_line_inner(byte))
    }

    fn byte_to_line_clamped(&self, byte: usize) -> usize {
        self.byte_to_line_inner(byte)
    }

    fn line_to_byte(&self, line: usize) -> Option<usize> {
        (line < self.rope.len_lines()).then(|| self.line_to_byte_inner(line))
    }

    fn line_to_byte_clamped(&self, line: usize) -> usize {
        self.line_to_byte_inner(line.min(self.rope.len_lines().saturating_sub(1)))
    }

    fn byte_to_char(&self, byte: usize) -> Option<usize> {
        (byte <= self.rope.len_bytes()).then(|| self.byte_to_char_inner(byte))
    }

    fn byte_to_char_clamped(&self, byte: usize) -> usize {
        self.byte_to_char_inner(byte)
    }

    fn char_to_byte(&self, char_idx: usize) -> Option<usize> {
        (char_idx <= self.rope.len_chars()).then(|| self.char_to_byte_inner(char_idx))
    }

    fn char_to_byte_clamped(&self, char_idx: usize) -> usize {
        self.char_to_byte_inner(char_idx)
    }

    fn char(&self, char_idx: usize) -> Option<char> {
        (char_idx < self.rope.len_chars()).then(|| self.char_inner(char_idx))
    }

    fn char_clamped(&self, char_idx: usize) -> char {
        let char_idx = char_idx.min(self.rope.len_chars().saturating_sub(1));
        if self.rope.len_chars() == 0 {
            '\0'
        } else {
            self.rope.char(char_idx)
        }
    }

    fn line(&self, line_idx: usize) -> Option<RopeSlice<'_>> {
        (line_idx < self.rope.len_lines()).then(|| self.rope.line(line_idx))
    }

    fn line_clamped(&self, line_idx: usize) -> RopeSlice<'_> {
        self.line_inner(line_idx)
    }

    fn lines_at(&self, byte: usize) -> Option<Lines<'_>> {
        (byte <= self.rope.len_bytes()).then(|| self.lines_at_inner(byte))
    }

    fn lines_at_clamped(&self, byte: usize) -> Lines<'_> {
        self.lines_at_inner(byte)
    }

    fn chunk_at(&self, byte: usize) -> Option<(&str, usize, usize, usize)> {
        if byte > self.rope.len_bytes() {
            None
        } else {
            let start_char = self.rope.byte_to_char(byte);
            let slice = self.rope.slice(start_char..self.rope.len_chars());
            let chunk = slice.chunks().next()?;
            Some((chunk, byte, 0, 0))
        }
    }

    fn byte_to_char_boundary(&self, byte: usize) -> usize {
        let byte = byte.min(self.rope.len_bytes());
        self.rope.byte_to_char_boundary_byte(byte)
    }

    fn slice_to_string(&self, start: usize, end: usize) -> Option<String> {
        self.slice_bounds_valid(start, end)
            .then(|| self.slice_inner(start, end).to_string())
    }

    fn slice(&self, start: usize, end: usize) -> Option<RopeSlice<'_>> {
        self.slice_bounds_valid(start, end)
            .then(|| self.slice_inner(start, end))
    }

    fn slice_clamped(&self, start: usize, end: usize) -> RopeSlice<'_> {
        self.slice_inner(start, end.min(self.rope.len_bytes()))
    }

    fn to_string(&self) -> String {
        self.rope.to_string()
    }

    fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    fn is_empty(&self) -> bool {
        self.rope.len_bytes() == 0
    }

    fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }
}
