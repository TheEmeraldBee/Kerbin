use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use ascii_forge::prelude::*;

use crate::*;

/// State managing and organizing drawing chunks (buffers).
///
/// `Chunks` provides a way to register and retrieve `InnerChunk` instances,
/// allowing different UI components to manage their own drawing areas.
/// Chunks are organized by a Z-index for layering.
#[derive(State, Default)]
pub struct Chunks {
    /// A vector of vectors, where the outer vector represents Z-layers
    /// and the inner vector holds `(position, InnerChunk)` pairs for that layer.
    pub buffers: Vec<Vec<(Vec2, Arc<RwLock<InnerChunk>>)>>,
    /// A map from state name (identifier for a chunk) to its `(z_index, inner_vec_index)` coordinates.
    chunk_idx_map: HashMap<String, (usize, usize)>,
}

impl Chunks {
    /// Clears all registered chunks and their associated buffers.
    ///
    /// This effectively resets the entire chunk management system.
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.chunk_idx_map.clear();
    }

    /// Registers a new chunk for drawing, identified by its state name.
    ///
    /// If a chunk with the given `C::static_name()` is already registered at the
    /// specified `z_index`, its existing entry might be updated. Otherwise, a new
    /// chunk is created and added. The size of the `InnerChunk`'s buffer is derived
    /// from the `rect`.
    ///
    /// # Type Parameters
    ///
    /// * `C`: The state type that implements `StateName` and `StaticState`. This type's
    ///        `static_name()` method provides a unique identifier for the chunk.
    ///
    /// # Arguments
    ///
    /// * `z_index`: The Z-index (layer) at which to draw this chunk. Higher indices
    ///              are drawn on top of lower indices.
    /// * `rect`: The `Rect` defining the position and size (width and height) of the chunk.
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

        if self.buffers[z_index].len() == coords.1 {
            // Add new chunk if not already present at this exact inner index
            self.buffers[z_index].push((
                pos.into(),
                Arc::new(RwLock::new(InnerChunk::new(Buffer::new(size)))),
            ));
        } else {
            // Otherwise, update existing chunk (e.g., if its dimensions changed)
            self.buffers[z_index][coords.1] = (
                pos.into(),
                Arc::new(RwLock::new(InnerChunk::new(Buffer::new(size)))),
            );
        }
    }

    /// Retrieves a registered chunk by its state name.
    ///
    /// This method allows access to the `InnerChunk` associated with a specific
    /// UI component or state, identified by its static name.
    ///
    /// # Type Parameters
    ///
    /// * `C`: The state type that implements `StateName` and `StaticState`, used to
    ///        identify the chunk via `C::static_name()`.
    ///
    /// # Returns
    ///
    /// An `Option<Arc<RwLock<InnerChunk>>>` containing a thread-safe reference to the
    /// chunk if found, or `None` if no chunk is registered under that name.
    pub fn get_chunk<C: StateName + StaticState>(&self) -> Option<Arc<RwLock<InnerChunk>>> {
        let id = C::static_name();

        let (ia, ib) = self.chunk_idx_map.get(&id)?;

        Some(self.buffers[*ia][*ib].1.clone())
    }
}
