use std::{collections::HashMap, sync::Arc};

use ascii_forge::prelude::*;
use tokio::sync::RwLock;

use crate::*;

/// State managing and organizing drawing chunks (buffers)
#[derive(State, Default)]
pub struct Chunks {
    /// Layered storage for drawing chunks
    pub buffers: Vec<Vec<(Vec2, Arc<RwLock<InnerChunk>>)>>,
    chunk_idx_map: HashMap<String, (usize, usize)>,
}

impl Chunks {
    /// Clears all registered chunks and their associated buffers
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.chunk_idx_map.clear();
    }

    /// Registers a new chunk for drawing
    pub fn register_chunk<C: StateName + StaticState>(&mut self, z_index: usize, rect: Rect) {
        let size = (rect.width, rect.height);
        let pos = (rect.x, rect.y);

        if self.buffers.len() <= z_index {
            self.buffers.resize(z_index + 1, Vec::default());
        }

        let coords = self
            .chunk_idx_map
            .entry(C::static_name())
            .or_insert((z_index, self.buffers[z_index].len()));

        // Create buffer filled with '\0' characters
        let mut buffer = Buffer::new(size);
        buffer.fill('\0');

        if self.buffers[z_index].len() == coords.1 {
            // Add new chunk if not already present at this exact inner index
            self.buffers[z_index]
                .push((pos.into(), Arc::new(RwLock::new(InnerChunk::new(buffer)))));
        } else {
            // Otherwise, update existing chunk (e.g., if its dimensions changed)
            self.buffers[z_index][coords.1] =
                (pos.into(), Arc::new(RwLock::new(InnerChunk::new(buffer))));
        }
    }

    /// Retrieves a registered chunk by its state name
    pub fn get_chunk<C: StateName + StaticState>(&self) -> Option<Arc<RwLock<InnerChunk>>> {
        let id = C::static_name();

        let (ia, ib) = self.chunk_idx_map.get(&id)?;

        Some(self.buffers[*ia][*ib].1.clone())
    }
}
