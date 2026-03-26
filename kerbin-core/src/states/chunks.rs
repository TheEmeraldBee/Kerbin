use std::{collections::HashMap, sync::Arc};

use ratatui::{buffer::Buffer, layout::Rect};
use tokio::sync::RwLock;

use crate::*;

/// State managing and organizing drawing chunks (buffers)
#[derive(State, Default)]
pub struct Chunks {
    /// Layered storage for drawing chunks, keyed by z-index then slot index
    pub buffers: Vec<Vec<Arc<RwLock<InnerChunk>>>>,
    chunk_idx_map: HashMap<String, (usize, usize, Rect)>,
    /// Tracks the count of registered indexed chunks per type name
    indexed_chunk_counts: HashMap<String, usize>,
}

impl Chunks {
    /// Clears all registered chunks and their associated buffers
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.chunk_idx_map.clear();
        self.indexed_chunk_counts.clear();
    }

    /// Internal helper: registers a chunk by a string key at the given z-index and rect
    fn register_by_key(&mut self, key: String, z_index: usize, rect: Rect) {
        if self.buffers.len() <= z_index {
            self.buffers.resize(z_index + 1, Vec::default());
        }

        let slot = self.buffers[z_index].len();
        let coords = self
            .chunk_idx_map
            .entry(key)
            .or_insert((z_index, slot, rect));
        // Update the stored rect each time the chunk is re-registered (layout may change)
        coords.2 = rect;

        let buffer = Buffer::empty(rect);

        if self.buffers[z_index].len() == coords.1 {
            self.buffers[z_index].push(Arc::new(RwLock::new(InnerChunk::new(buffer))));
        } else {
            self.buffers[z_index][coords.1] = Arc::new(RwLock::new(InnerChunk::new(buffer)));
        }
    }

    /// Registers a new chunk for drawing at the given z-index and rect
    pub fn register_chunk<C: StateName + StaticState>(&mut self, z_index: usize, rect: Rect) {
        self.register_by_key(C::static_name(), z_index, rect);
    }

    /// Retrieves a registered chunk by its state name
    pub fn get_chunk<C: StateName + StaticState>(&self) -> Option<Arc<RwLock<InnerChunk>>> {
        let id = C::static_name();
        let (ia, ib, _) = self.chunk_idx_map.get(&id)?;
        Some(self.buffers[*ia][*ib].clone())
    }

    /// Returns the last-registered rect for a chunk by name, if any
    pub fn rect_for_chunk(&self, name: &str) -> Option<Rect> {
        self.chunk_idx_map.get(name).map(|(_, _, rect)| *rect)
    }

    /// Registers a chunk for a specific pane index at the given z-index and rect.
    /// Indexed chunks use a synthetic key `"TypeName[index]"` separate from named chunks.
    pub fn register_indexed_chunk<C: StateName + StaticState>(
        &mut self,
        index: usize,
        z_index: usize,
        rect: Rect,
    ) {
        let key = format!("{}[{}]", C::static_name(), index);
        self.register_by_key(key, z_index, rect);

        let count = self
            .indexed_chunk_counts
            .entry(C::static_name())
            .or_insert(0);
        if *count <= index {
            *count = index + 1;
        }
    }

    /// Retrieves an indexed chunk by type and pane index
    pub fn get_indexed_chunk<C: StateName + StaticState>(
        &self,
        index: usize,
    ) -> Option<Arc<RwLock<InnerChunk>>> {
        let key = format!("{}[{}]", C::static_name(), index);
        let (ia, ib, _) = self.chunk_idx_map.get(&key)?;
        Some(self.buffers[*ia][*ib].clone())
    }

    /// Returns the rect for an indexed chunk by type and pane index
    pub fn rect_for_indexed_chunk<C: StateName + StaticState>(&self, index: usize) -> Option<Rect> {
        let key = format!("{}[{}]", C::static_name(), index);
        self.chunk_idx_map.get(&key).map(|(_, _, rect)| *rect)
    }

    /// Returns the number of registered indexed chunks for the given type
    pub fn indexed_count<C: StateName + StaticState>(&self) -> usize {
        self.indexed_chunk_counts
            .get(&C::static_name())
            .copied()
            .unwrap_or(0)
    }
}
