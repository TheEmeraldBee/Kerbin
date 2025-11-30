use crate::TextBuffer;

/// Is emitted when a buffer is saved
pub struct SaveEvent {
    /// The path the file was saved to
    pub path: String,
}

/// Is emitted when a buffer is closed
pub struct CloseEvent {
    /// The contained buffer
    pub buffer: TextBuffer,
}
