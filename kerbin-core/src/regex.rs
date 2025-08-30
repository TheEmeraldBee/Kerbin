use regex_cursor::*;

#[derive(Clone, Copy)]
enum Pos {
    ChunkStart,
    ChunkEnd,
}

#[derive(Clone)]
pub struct RopeyCursor<'a> {
    iter: ropey::iter::Chunks<'a>,
    current: &'a [u8],
    pos: Pos,
    len: usize,
    offset: usize,
}

impl<'a> RopeyCursor<'a> {
    pub fn new(slice: ropey::RopeSlice<'a>) -> Self {
        let iter = slice.chunks();
        let mut res = Self {
            current: &[],
            iter,
            pos: Pos::ChunkEnd,
            len: slice.len(),
            offset: 0,
        };
        res.advance();
        res
    }

    pub fn at(slice: ropey::RopeSlice<'a>, at: usize) -> Self {
        let (iter, offset) = slice.chunks_at(at);
        if offset == slice.len() {
            let mut res = Self {
                current: &[],
                iter,
                pos: Pos::ChunkStart,
                len: slice.len(),
                offset,
            };
            res.backtrack();
            res
        } else {
            let mut res = Self {
                current: &[],
                iter,
                pos: Pos::ChunkEnd,
                len: slice.len(),
                offset,
            };
            res.advance();
            res
        }
    }
}

impl Cursor for RopeyCursor<'_> {
    fn chunk(&self) -> &[u8] {
        self.current
    }

    fn advance(&mut self) -> bool {
        match self.pos {
            Pos::ChunkStart => {
                self.iter.next();
                self.pos = Pos::ChunkEnd;
            }
            Pos::ChunkEnd => (),
        }
        for next in self.iter.by_ref() {
            if next.is_empty() {
                continue;
            }
            self.offset += self.current.len();
            self.current = next.as_bytes();
            return true;
        }
        false
    }

    fn backtrack(&mut self) -> bool {
        match self.pos {
            Pos::ChunkStart => {}
            Pos::ChunkEnd => {
                self.iter.prev();
                self.pos = Pos::ChunkStart;
            }
        }
        while let Some(prev) = self.iter.prev() {
            if prev.is_empty() {
                continue;
            }
            self.offset -= prev.len();
            self.current = prev.as_bytes();
            return true;
        }
        false
    }

    fn utf8_aware(&self) -> bool {
        true
    }

    fn total_bytes(&self) -> Option<usize> {
        Some(self.len)
    }

    fn offset(&self) -> usize {
        self.offset
    }
}
