use ropey::{RopeSlice, iter::Lines};

use crate::TextBuffer;
use crate::rope_exts::RopeExts;

/// Extension trait for safe rope operations on TextBuffer
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

impl SafeRopeAccess for TextBuffer {
    fn byte_to_line(&self, byte: usize) -> Option<usize> {
        if byte > self.rope.len_bytes() {
            None
        } else {
            Some(self.rope.byte_to_line(byte))
        }
    }

    fn byte_to_line_clamped(&self, byte: usize) -> usize {
        let byte = byte.min(self.rope.len_bytes());
        self.rope.byte_to_line(byte)
    }

    fn line_to_byte(&self, line: usize) -> Option<usize> {
        if line >= self.rope.len_lines() {
            None
        } else {
            Some(self.rope.char_to_byte(self.rope.line_to_char(line)))
        }
    }

    fn line_to_byte_clamped(&self, line: usize) -> usize {
        let total_lines = self.rope.len_lines();
        let line = line.min(total_lines.saturating_sub(1));
        self.rope.char_to_byte(self.rope.line_to_char(line))
    }

    fn byte_to_char(&self, byte: usize) -> Option<usize> {
        if byte > self.rope.len_bytes() {
            None
        } else {
            Some(self.rope.byte_to_char(byte))
        }
    }

    fn byte_to_char_clamped(&self, byte: usize) -> usize {
        let byte = byte.min(self.rope.len_bytes());
        self.rope.byte_to_char(byte)
    }

    fn char_to_byte(&self, char_idx: usize) -> Option<usize> {
        if char_idx > self.rope.len_chars() {
            None
        } else {
            Some(self.rope.char_to_byte(char_idx))
        }
    }

    fn char_to_byte_clamped(&self, char_idx: usize) -> usize {
        let char_idx = char_idx.min(self.rope.len_chars());
        self.rope.char_to_byte(char_idx)
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
        if self.rope.len_chars() == 0 {
            '\0'
        } else {
            self.rope.char(char_idx)
        }
    }

    fn line(&self, line_idx: usize) -> Option<RopeSlice<'_>> {
        if line_idx >= self.rope.len_lines() {
            None
        } else {
            Some(self.rope.line(line_idx))
        }
    }

    fn line_clamped(&self, line_idx: usize) -> RopeSlice<'_> {
        let total_lines = self.rope.len_lines();
        let line_idx = line_idx.min(total_lines.saturating_sub(1));
        self.rope.line(line_idx)
    }

    fn lines_at(&self, byte: usize) -> Option<Lines<'_>> {
        if byte > self.rope.len_bytes() {
            None
        } else {
            Some(self.rope.lines_at(self.rope.byte_to_char(byte)))
        }
    }

    fn lines_at_clamped(&self, byte: usize) -> Lines<'_> {
        let byte = byte.min(self.rope.len_bytes());
        self.rope.lines_at(self.rope.byte_to_char(byte))
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
        let len = self.rope.len_bytes();
        if start > len || end > len || start > end {
            return None;
        }
        Some(
            self.rope
                .slice(self.rope.byte_to_char(start)..self.rope.byte_to_char(end))
                .to_string(),
        )
    }

    fn slice(&self, start: usize, end: usize) -> Option<RopeSlice<'_>> {
        let len = self.rope.len_bytes();
        if start > len || end > len || start > end {
            return None;
        }
        Some(self.rope.slice(self.rope.byte_to_char(start)..self.rope.byte_to_char(end)))
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
