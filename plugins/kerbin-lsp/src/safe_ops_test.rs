#[cfg(test)]
mod tests {
    use kerbin_core::{TextBuffer, LineType};
    use ropey::Rope;

    #[test]
    fn test_rope_conversion_safety() {
        let text = "Hello\nWorld";
        let rope = Rope::from_str(text);
        
        // Emulate the logic in diagnostics.rs
        let to_byte = |line: usize, col: usize| -> usize {
            let total_lines = rope.len_lines(LineType::LF_CR);
            let line = line.min(total_lines.saturating_sub(1));
            
            let line_start_byte = rope.line_to_byte_idx(line, LineType::LF_CR);
            let line_start_char = rope.byte_to_char_idx(line_start_byte);
            
            let line_len_chars = rope.line(line, LineType::LF_CR).len_chars();
            
            // Clamp col to line length
            let col = col.min(line_len_chars);
            
            let global_char = line_start_char + col;
            // Clamp to total chars
            let global_char = global_char.min(rope.len_chars());
            
            rope.char_to_byte_idx(global_char)
        };

        // Normal case
        // Line 0: "Hello\n" (6 chars)
        // Col 0 -> "H" -> byte 0
        assert_eq!(to_byte(0, 0), 0);
        // Col 5 -> "\n" -> byte 5
        assert_eq!(to_byte(0, 5), 5);
        
        // Out of bounds line
        // Should clamp to last line (1: "World")
        // "World" starts at byte 6
        assert_eq!(to_byte(100, 0), 6); 

        // Out of bounds col
        // Line 0 length is 6. Col 100 should clamp to 6.
        // char 6 is start of "World" (byte 6).
        assert_eq!(to_byte(0, 100), 6);
        
        // Line 1: "World" (5 chars). Starts at byte 6.
        // Col 5 -> end of string (byte 11)
        assert_eq!(to_byte(1, 5), 11);
    }
}
