use ropey::Rope;

pub trait RopeExts {
    /// Converts the given byte index into another byte index that sits on a valid char boundary
    fn byte_to_char_boundary_byte(&self, byte: usize) -> usize;
}

impl RopeExts for Rope {
    fn byte_to_char_boundary_byte(&self, byte: usize) -> usize {
        let char_idx = self.byte_to_char_idx(byte);
        self.char_to_byte_idx(char_idx)
    }
}
