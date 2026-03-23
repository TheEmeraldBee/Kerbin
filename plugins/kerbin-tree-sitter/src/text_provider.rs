use ropey::Rope;
use tree_sitter::TextProvider;

pub struct TextProviderRope<'a>(pub &'a Rope);

impl<'a> TextProvider<&'a [u8]> for &'a TextProviderRope<'a> {
    type I = ChunksBytes<'a>;
    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        let mut byte_range = node.byte_range();

        if self.0.len_bytes() <= byte_range.start {
            return ChunksBytes(None);
        }

        byte_range.end = byte_range.end.min(self.0.len_bytes());

        let char_start = self.0.byte_to_char(byte_range.start);
        let char_end = self.0.byte_to_char(byte_range.end);

        ChunksBytes(Some(self.0.slice(char_start..char_end).chunks()))
    }
}

pub struct ChunksBytes<'a>(Option<ropey::iter::Chunks<'a>>);

impl<'a> Iterator for ChunksBytes<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        self.0.as_mut()?.next().map(|s| s.as_bytes())
    }
}
