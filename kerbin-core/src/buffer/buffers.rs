use std::sync::Arc;

use crate::{Theme, get_canonical_path_with_non_existent};

use super::TextBuffer;
use ascii_forge::prelude::*;
use kerbin_macros::State;
use kerbin_state_machine::storage::*;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

/// Stores all text buffers managed by the editor, along with their unique paths and selection state.
#[derive(Default, State)]
pub struct Buffers {
    /// The index of the currently selected buffer in the `buffers` vector.
    pub selected_buffer: usize,

    /// The horizontal scroll offset of the tab-bar (bufferline) in characters.
    pub tab_scroll: usize,

    /// The internal storage of `TextBuffer` instances. Each buffer is wrapped in an
    /// `Arc<RwLock>` for concurrent access.
    pub buffers: Vec<Arc<RwLock<TextBuffer>>>,

    /// The list of unique, shortened paths corresponding to each buffer.
    /// These paths are generated to be distinguishable in the UI.
    pub buffer_paths: Vec<String>,
}

impl Buffers {
    /// Returns a read lock to the currently selected buffer.
    pub async fn cur_buffer(&self) -> OwnedRwLockReadGuard<TextBuffer> {
        self.buffers[self.selected_buffer]
            .clone()
            .read_owned()
            .await
    }

    /// Returns a read lock to the currently selected buffer.
    pub async fn cur_buffer_mut(&mut self) -> OwnedRwLockWriteGuard<TextBuffer> {
        self.buffers[self.selected_buffer]
            .clone()
            .write_owned()
            .await
    }

    /// Changes the selected buffer by a given signed distance.
    ///
    /// The selection will not wrap around the ends of the buffer list.
    ///
    /// # Arguments
    ///
    /// * `dist`: The signed distance to move the selection (e.g., `1` for next, `-1` for previous).
    pub fn change_buffer(&mut self, dist: isize) {
        self.selected_buffer = self
            .selected_buffer
            .saturating_add_signed(dist)
            .clamp(0, self.buffers.len() - 1);
    }

    /// Sets the selected buffer to a specific index.
    ///
    /// The provided index will be clamped to ensure it's within the valid range
    /// of available buffers.
    ///
    /// # Arguments
    ///
    /// * `id`: The index of the buffer to select.
    pub fn set_selected_buffer(&mut self, id: usize) {
        self.selected_buffer = id.clamp(0, self.buffers.len() - 1);
    }

    /// Closes the buffer at the given index.
    ///
    /// If closing the buffer results in no open buffers, a new scratch buffer
    /// is automatically created to ensure there's always at least one buffer.
    /// The `selected_buffer` is adjusted accordingly if the closed buffer was active.
    ///
    /// # Arguments
    ///
    /// * `idx`: The index of the buffer to close.
    pub fn close_buffer(&mut self, idx: usize) {
        self.buffers.remove(idx);
        if self.buffers.is_empty() {
            self.buffers
                .push(Arc::new(RwLock::new(TextBuffer::scratch())));
        }

        self.change_buffer(0); // Adjust selected_buffer to remain valid
    }

    /// Opens a buffer with the given file path.
    ///
    /// If a buffer with the canonicalized version of the provided `path` is already
    /// open, that existing buffer is selected instead of opening a duplicate.
    /// Otherwise, a new `TextBuffer` is created, opened, and set as the selected buffer.
    ///
    /// # Arguments
    ///
    /// * `path`: The file path `String` to open.
    ///
    /// # Returns
    ///
    /// The index of the newly opened or selected buffer.
    pub async fn open(&mut self, path: String) -> std::io::Result<usize> {
        let check_path = get_canonical_path_with_non_existent(&path)
            .to_str()
            .unwrap()
            .to_string();

        let mut found_buffer_id: Option<usize> = None;

        for (i, buffer_arc) in self.buffers.iter().enumerate() {
            let buffer_read = buffer_arc.read().await;

            if buffer_read.path == check_path {
                found_buffer_id = Some(i);
                break;
            }
        }

        if let Some(buffer_id) = found_buffer_id {
            self.set_selected_buffer(buffer_id);
            Ok(buffer_id)
        } else {
            let new_buffer = Arc::new(RwLock::new(TextBuffer::open(path)?));
            self.buffers.push(new_buffer);
            let new_buffer_id = self.buffers.len() - 1;
            self.set_selected_buffer(new_buffer_id);

            Ok(new_buffer_id)
        }
    }

    /// Renders the bufferline (tab bar) into the provided `Buffer`.
    ///
    /// This method displays the unique paths of all open buffers, highlighting
    /// the currently selected one and handling horizontal scrolling.
    ///
    /// # Arguments
    ///
    /// * `buffer`: A mutable reference to the `Buffer` where the bufferline should be drawn.
    /// * `theme`: A reference to the `Theme` for styling the bufferline elements.
    pub async fn render_bufferline(&self, buffer: &mut Buffer, theme: &Theme) {
        let mut current_char_offset = 0;

        for (i, short_path) in self.buffer_paths.iter().enumerate() {
            // Format the title with padding
            let title = format!(
                "   {} {} ",
                short_path,
                match self.buffers[i].read().await.dirty {
                    true => "*",
                    false => " ",
                }
            );
            let title_width = title.chars().count();

            // Calculate the visible range of the bufferline chunk
            let visible_range_start = self.tab_scroll;
            let visible_range_end = self.tab_scroll + buffer.size().x as usize;
            // Calculate the start and end of the current tab
            let tab_range_start = current_char_offset;
            let tab_range_end = current_char_offset + title_width;

            // Determine the style based on whether this is the selected buffer
            let style = if i == self.selected_buffer {
                theme.get_fallback_default(["ui.bufferline.selected", "ui.bufferline", "ui.text"])
            } else {
                theme.get_fallback_default(["ui.bufferline", "ui.text"])
            };

            // Calculate the overlap between the tab and the visible area
            let overlap_start = visible_range_start.max(tab_range_start);
            let overlap_end = visible_range_end.min(tab_range_end);

            // Render only the visible part of the tab
            if overlap_start < overlap_end {
                let slice_start = overlap_start - tab_range_start;
                let slice_len = overlap_end - overlap_start;
                let visible_part: String =
                    title.chars().skip(slice_start).take(slice_len).collect();

                let render_x = (overlap_start - self.tab_scroll) as u16;
                render!(buffer, vec2(render_x, 0) => [ StyledContent::new(style, visible_part) ]);
            }

            current_char_offset += title_width;
        }
    }

    /// Updates the unique, shortened paths for all currently open buffers.
    ///
    /// This function re-calculates the `buffer_paths` vector based on the
    /// full paths of the `TextBuffer`s, ensuring they are unique and readable.
    pub async fn update_paths(&mut self) {
        let mut paths: Vec<String> = Vec::with_capacity(self.buffers.len());

        for buffer_arc in self.buffers.iter() {
            let buffer_read = buffer_arc.read().await;

            paths.push(buffer_read.path.clone());
        }

        let unique_paths = get_unique_paths(paths.into_iter(), self.buffers.len());
        self.buffer_paths = unique_paths;
    }

    /// Returns the unique (shortened) path of the buffer at the given index.
    ///
    /// # Arguments
    ///
    /// * `idx`: The index of the buffer whose unique path is requested.
    ///
    /// # Returns
    ///
    /// An `Option<String>` containing the unique path if the index is valid,
    /// otherwise `None`.
    pub fn unique_path_of(&self, idx: usize) -> Option<String> {
        self.buffer_paths.get(idx).cloned()
    }
}

/// Helper function that takes an iterator of full paths and generates a list
/// of unique, readable shortened paths.
///
/// If paths share common prefixes, it will attempt to truncate them to the shortest
/// possible unique representation.
///
/// # Arguments
///
/// * `paths`: An iterator yielding `String` representations of full file paths.
/// * `len`: The total number of paths (expected length of the output vector).
///
/// # Returns
///
/// A `Vec<String>` containing the unique, shortened paths.
fn get_unique_paths(paths: impl Iterator<Item = String>, len: usize) -> Vec<String> {
    if len == 0 {
        return vec![];
    }

    let path_components: Vec<Vec<String>> = paths
        .map(|p| p.split('/').map(|x| x.to_string()).collect())
        .collect();

    let mut truncated_paths = path_components
        .iter()
        .map(|_| String::new())
        .collect::<Vec<_>>();

    for i in 0..len {
        let mut depth = 1;
        loop {
            // Take components from the end to make it unique
            let truncated_parts: Vec<String> = path_components[i]
                .iter()
                .rev()
                .take(depth)
                .rev()
                .cloned()
                .collect();
            let truncated = truncated_parts.join("/");

            // Check if this truncated path is unique among all *other* paths
            let is_unique = path_components
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i) // Compare only with other paths
                .all(|(_, other_components)| {
                    let other_truncated = other_components
                        .iter()
                        .rev()
                        .take(depth)
                        .rev()
                        .cloned()
                        .collect::<Vec<String>>()
                        .join("/");
                    truncated != other_truncated
                });

            // If unique, or we've used all components, stop for this path
            if is_unique || depth >= path_components[i].len() {
                truncated_paths[i] = truncated;
                break;
            }
            depth += 1; // Increase depth (take more components from the end)
        }
    }

    truncated_paths
}
