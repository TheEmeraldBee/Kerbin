use regex_cursor::*;

#[derive(Clone, Copy)]
enum Pos {
    ChunkStart,
    ChunkEnd,
}

/// A cursor implementation for ropey::RopeSlice that allows efficient iteration over chunks of text
#[derive(Clone)]
pub struct RopeyCursor<'a> {
    iter: ropey::iter::Chunks<'a>,
    current: &'a [u8],
    pos: Pos,
    len: usize,
    offset: usize,
}

impl<'a> RopeyCursor<'a> {
    /// Creates a new RopeyCursor starting from the beginning of the given RopeSlice
    pub fn new(slice: ropey::RopeSlice<'a>) -> Self {
        let iter = slice.chunks();
        let mut res = Self {
            current: &[],
            iter,
            pos: Pos::ChunkEnd, // Initialize to ChunkEnd to force an initial advance
            len: slice.len(),   // Use len_bytes for total bytes
            offset: 0,
        };
        res.advance(); // Advance to the first actual chunk
        res
    }

    /// Creates a new RopeyCursor positioned at a specific byte offset within the RopeSlice
    pub fn at(slice: ropey::RopeSlice<'a>, at: usize) -> Self {
        let (iter, offset_in_chunk) = slice.chunks_at(at);
        let len = slice.len();

        if at == len {
            // If `at` is exactly at the end, backtrack to get the last chunk
            let mut res = Self {
                current: &[], // Will be set by backtrack
                iter,
                pos: Pos::ChunkStart, // Force backtrack to find the previous chunk
                len,
                offset: at, // Current offset is `at`
            };
            res.backtrack();
            res
        } else {
            // Otherwise, advance to find the chunk containing `at`
            let mut res = Self {
                current: &[], // Will be set by advance
                iter,
                pos: Pos::ChunkEnd, // Force advance to find the next chunk
                len,
                offset: at - offset_in_chunk, // Adjust offset to chunk start
            };
            res.advance(); // Advance to the correct chunk
            res
        }
    }
}

impl Cursor for RopeyCursor<'_> {
    /// Returns the current byte slice chunk that the cursor is pointing to
    fn chunk(&self) -> &[u8] {
        self.current
    }

    /// Advances the cursor to the next non-empty chunk
    fn advance(&mut self) -> bool {
        match self.pos {
            Pos::ChunkStart => {
                // processing that chunk and then moving to the next from the iterator.
                // The `iter.next()` here effectively consumes the chunk we were `ChunkStart` of.
                self.iter.next();
                self.pos = Pos::ChunkEnd;
            }
            Pos::ChunkEnd => {
                // If we were at the end, `iter` is already correctly positioned
                // to give the *next* chunk. Do nothing here.
            }
        }

        // Iterate through remaining chunks from the ropey iterator
        for next in self.iter.by_ref() {
            if next.is_empty() {
                continue; // Skip empty chunks
            }
            // Update offset: add the length of the chunk we just moved *from*.
            // Note: `self.current.len()` is the length of the *previous* chunk.
            self.offset += self.current.len();
            self.current = next.as_bytes(); // Set the new current chunk
            self.pos = Pos::ChunkEnd; // The cursor is now at the end of this new chunk
            return true;
        }
        false // No more chunks
    }

    /// Moves the cursor to the previous non-empty chunk
    fn backtrack(&mut self) -> bool {
        // If already at ChunkStart, `iter.prev()` would skip the current chunk,
        // so we don't need to call it again.
        match self.pos {
            Pos::ChunkStart => {
                // If we were at the start of a chunk, moving backward means
                // `iter.prev()` would give us the chunk *before* the one we're currently holding.
                // Do nothing here, `iter.prev()` below will handle it.
            }
            Pos::ChunkEnd => {
                // If we were at the end of a chunk, we need to conceptually move
                // backward past the *current* chunk, so we tell `iter` to go back one.
                self.iter.prev();
                self.pos = Pos::ChunkStart;
            }
        }

        // Iterate backward through chunks from the ropey iterator
        while let Some(prev) = self.iter.prev() {
            if prev.is_empty() {
                continue; // Skip empty chunks
            }
            // Update offset: subtract the length of the chunk we are moving *to*.
            self.offset -= prev.len();
            self.current = prev.as_bytes(); // Set the new current chunk
            self.pos = Pos::ChunkStart; // The cursor is now at the start of this new chunk
            return true;
        }
        false // No more previous chunks
    }

    /// Indicates whether the cursor is aware of UTF-8 boundaries
    fn utf8_aware(&self) -> bool {
        true
    }

    /// Returns the total number of bytes in the underlying RopeSlice
    fn total_bytes(&self) -> Option<usize> {
        Some(self.len)
    }

    /// Returns the current byte offset of the cursor from the beginning of the RopeSlice
    fn offset(&self) -> usize {
        self.offset
    }
}
