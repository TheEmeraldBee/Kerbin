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
}

impl Chunks {
    /// Clears all registered chunks and their associated buffers
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.chunk_idx_map.clear();
    }

    /// Registers a new chunk for drawing at the given z-index and rect
    pub fn register_chunk<C: StateName + StaticState>(&mut self, z_index: usize, rect: Rect) {
        if self.buffers.len() <= z_index {
            self.buffers.resize(z_index + 1, Vec::default());
        }

        let name = C::static_name();
        let slot = self.buffers[z_index].len();
        let coords = self
            .chunk_idx_map
            .entry(name)
            .or_insert((z_index, slot, rect));
        // Update the stored rect each time the chunk is re-registered (layout may change)
        coords.2 = rect;

        // Create empty buffer covering the rect (position encoded in buffer.area)
        let buffer = Buffer::empty(rect);

        if self.buffers[z_index].len() == coords.1 {
            self.buffers[z_index].push(Arc::new(RwLock::new(InnerChunk::new(buffer))));
        } else {
            self.buffers[z_index][coords.1] = Arc::new(RwLock::new(InnerChunk::new(buffer)));
        }
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
}
