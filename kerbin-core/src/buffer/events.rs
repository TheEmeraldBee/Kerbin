use std::sync::Arc;

use tokio::sync::RwLock;

use crate::KerbinBuffer;

/// Is emitted when a buffer is saved
pub struct SaveEvent {
    /// The path the file was saved to
    pub path: String,
}

/// Is emitted when a buffer is closed
pub struct CloseEvent {
    /// The closed buffer (still locked in the Arc)
    pub buffer: Arc<RwLock<dyn KerbinBuffer>>,
}
