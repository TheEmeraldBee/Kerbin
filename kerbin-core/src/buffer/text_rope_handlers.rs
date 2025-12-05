use ropey::{LineType, RopeSlice, iter::Lines};

use crate::TextBuffer;
use crate::rope_exts::RopeExts;

/// Extension trait for safe rope operations on TextBuffer.
///
/// This provides helper methods that automatically handle bounds checking
/// and clamping to prevent panics when accessing the underlying rope.
pub trait SafeRopeAccess {
    /// Safely gets the line index for a byte index.
    /// Returns None if the byte is out of bounds.
    fn byte_to_line(&self, byte: usize) -> Option<usize>;

    /// Gets the line index for a byte index, clamping to the valid range.
    fn byte_to_line_clamped(&self, byte: usize) -> usize;

    /// Safely gets the byte index for a line index.
    /// Returns None if the line is out of bounds.
    fn line_to_byte(&self, line: usize) -> Option<usize>;

    /// Gets the byte index for a line index, clamping to the valid range.
    fn line_to_byte_clamped(&self, line: usize) -> usize;

    /// Safely gets the char index for a byte index.
    /// Returns None if the byte is out of bounds.
    fn byte_to_char(&self, byte: usize) -> Option<usize>;

    /// Gets the char index for a byte index, clamping to the valid range.
    fn byte_to_char_clamped(&self, byte: usize) -> usize;

    /// Safely gets the byte index for a char index.
    /// Returns None if the char is out of bounds.
    fn char_to_byte(&self, char_idx: usize) -> Option<usize>;

    /// Gets the byte index for a char index, clamping to the valid range.
    fn char_to_byte_clamped(&self, char_idx: usize) -> usize;

    /// Safely gets a character at the specified index.
    /// Returns None if the index is out of bounds.
    fn char(&self, char_idx: usize) -> Option<char>;

    /// Gets a character at the specified index, clamping to the valid range.
    fn char_clamped(&self, char_idx: usize) -> char;

    /// Safely gets a line slice.
    /// Returns None if the index is out of bounds.
    fn line(&self, line_idx: usize) -> Option<RopeSlice<'_>>;

    /// Gets a line slice, clamping to the last line.
    fn line_clamped(&self, line_idx: usize) -> RopeSlice<'_>;

    /// Safely gets an iterator over lines starting at byte.
    /// Returns None if byte is out of bounds.
    fn lines_at(&self, byte: usize) -> Option<Lines<'_>>;

    /// Gets an iterator over lines starting at byte, clamping byte to len.
    fn lines_at_clamped(&self, byte: usize) -> Lines<'_>;

    /// Safely gets the chunk at the specified byte.
    fn chunk_at(&self, byte: usize) -> Option<(&str, usize, usize, usize)>;

    /// Gets the byte offset of the char boundary at or before the given byte, clamping input.
    fn byte_to_char_boundary(&self, byte: usize) -> usize;

    /// Safely slices the rope.
    /// Returns None if the range is invalid.
    fn slice_to_string(&self, start: usize, end: usize) -> Option<String>;

    /// Safely slices the rope, returning a RopeSlice.
    fn slice(&self, start: usize, end: usize) -> Option<RopeSlice<'_>>;

    /// Returns the entire buffer content as a String.
    fn to_string(&self) -> String;

    /// Returns the length of the buffer in bytes.
    fn len(&self) -> usize;

    /// Returns whether the buffer is empty or not.
    fn is_empty(&self) -> bool;

    /// Returns the length of the buffer in chars.
    fn len_chars(&self) -> usize;

    /// Returns the length of the buffer in lines.
    fn len_lines(&self) -> usize;
}

impl SafeRopeAccess for TextBuffer {
    fn byte_to_line(&self, byte: usize) -> Option<usize> {
        if byte > self.rope.len() {
            None
        } else {
            // byte_to_line_idx panics if byte > len, so check first
            Some(self.rope.byte_to_line_idx(byte, LineType::LF_CR))
        }
    }

    fn byte_to_line_clamped(&self, byte: usize) -> usize {
        let byte = byte.min(self.rope.len());
        self.rope.byte_to_line_idx(byte, LineType::LF_CR)
    }

    fn line_to_byte(&self, line: usize) -> Option<usize> {
        if line >= self.rope.len_lines(LineType::LF_CR) {
            None
        } else {
            // line_to_byte_idx panics if line >= len_lines
            Some(self.rope.line_to_byte_idx(line, LineType::LF_CR))
        }
    }

    fn line_to_byte_clamped(&self, line: usize) -> usize {
        let total_lines = self.rope.len_lines(LineType::LF_CR);
        let line = line.min(total_lines.saturating_sub(1));
        self.rope.line_to_byte_idx(line, LineType::LF_CR)
    }

    fn byte_to_char(&self, byte: usize) -> Option<usize> {
        if byte > self.rope.len() {
            None
        } else {
            Some(self.rope.byte_to_char_idx(byte))
        }
    }

    fn byte_to_char_clamped(&self, byte: usize) -> usize {
        let byte = byte.min(self.rope.len());
        self.rope.byte_to_char_idx(byte)
    }

    fn char_to_byte(&self, char_idx: usize) -> Option<usize> {
        if char_idx > self.rope.len_chars() {
            None
        } else {
            Some(self.rope.char_to_byte_idx(char_idx))
        }
    }

    fn char_to_byte_clamped(&self, char_idx: usize) -> usize {
        let char_idx = char_idx.min(self.rope.len_chars());
        self.rope.char_to_byte_idx(char_idx)
    }

    fn char(&self, char_idx: usize) -> Option<char> {
        if char_idx >= self.rope.len_chars() {
            None
        } else {
            Some(self.rope.char(char_idx))
        }
    }

    fn char_clamped(&self, char_idx: usize) -> char {
        let char_idx = char_idx.min(self.rope.len_chars().saturating_sub(1));
        // Rope::char panics if index out of bounds, so ensure we handle empty rope or clamp
        if self.rope.len_chars() == 0 {
            // Assuming empty rope has no chars. Return \0 or panic?
            // ropey panics on index out of bounds.
            // If rope is empty, char_idx 0 is out of bounds.
            // We should probably return a safe default or handle empty.
            // Defaulting to null char for safety in clamped contexts.
            '\0'
        } else {
            self.rope.char(char_idx)
        }
    }

    fn line(&self, line_idx: usize) -> Option<RopeSlice<'_>> {
        if line_idx >= self.rope.len_lines(LineType::LF_CR) {
            None
        } else {
            Some(self.rope.line(line_idx, LineType::LF_CR))
        }
    }

    fn line_clamped(&self, line_idx: usize) -> RopeSlice<'_> {
        let total_lines = self.rope.len_lines(LineType::LF_CR);
        let line_idx = line_idx.min(total_lines.saturating_sub(1));
        self.rope.line(line_idx, LineType::LF_CR)
    }

    fn lines_at(&self, byte: usize) -> Option<Lines<'_>> {
        if byte > self.rope.len() {
            None
        } else {
            Some(self.rope.lines_at(byte, LineType::LF_CR))
        }
    }

    fn lines_at_clamped(&self, byte: usize) -> Lines<'_> {
        let byte = byte.min(self.rope.len());
        self.rope.lines_at(byte, LineType::LF_CR)
    }

    fn chunk_at(&self, byte: usize) -> Option<(&str, usize, usize, usize)> {
        if byte > self.rope.len() {
            None
        } else {
            // Emulate chunk_at_byte using slicing to be safe and compatible
            // Returning (chunk, byte, 0, 0) means the chunk starts exactly at `byte`.
            // The consumer in state.rs uses (byte - start_byte) which becomes 0.
            let slice = self.rope.slice(byte..self.rope.len());
            let chunk = slice.chunks().next()?;
            Some((chunk, byte, 0, 0))
        }
    }

    fn byte_to_char_boundary(&self, byte: usize) -> usize {
        let byte = byte.min(self.rope.len());
        self.rope.byte_to_char_boundary_byte(byte)
    }

    fn slice_to_string(&self, start: usize, end: usize) -> Option<String> {
        let len = self.rope.len();
        if start > len || end > len || start > end {
            return None;
        }
        Some(self.rope.slice(start..end).to_string())
    }

    fn slice(&self, start: usize, end: usize) -> Option<RopeSlice<'_>> {
        let len = self.rope.len();
        if start > len || end > len || start > end {
            return None;
        }
        Some(self.rope.slice(start..end))
    }

    fn to_string(&self) -> String {
        self.rope.to_string()
    }

    fn len(&self) -> usize {
        self.rope.len()
    }

    fn is_empty(&self) -> bool {
        self.rope.len() == 0
    }

    fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    fn len_lines(&self) -> usize {
        self.rope.len_lines(LineType::LF_CR)
    }
}
