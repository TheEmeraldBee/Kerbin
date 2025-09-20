use std::sync::{Arc, RwLock};

use crate::{BufferlineChunk, Chunk, Theme, WindowState, get_canonical_path_with_non_existent};

use super::TextBuffer;
use ascii_forge::prelude::*;
use kerbin_macros::State;
use kerbin_state_machine::storage::*;
use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};
use ropey::LineType;

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
    /// Returns an `Arc<RwLock<TextBuffer>>` to the currently selected buffer.
    ///
    /// This allows external systems to acquire a read or write lock on the
    /// active buffer for modifications or inspections.
    ///
    /// # Returns
    ///
    /// An `Arc<RwLock<TextBuffer>>` pointing to the selected buffer.
    pub fn cur_buffer(&self) -> Arc<RwLock<TextBuffer>> {
        self.buffers[self.selected_buffer].clone()
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
    pub fn open(&mut self, path: String) -> usize {
        let check_path = get_canonical_path_with_non_existent(&path)
            .to_str()
            .unwrap()
            .to_string();
        if let Some(buffer_id) = self.buffers.iter().enumerate().find_map(|(i, x)| {
            if x.read().unwrap().path == check_path {
                Some(i)
            } else {
                None
            }
        }) {
            self.set_selected_buffer(buffer_id);
        } else {
            self.buffers
                .push(Arc::new(RwLock::new(TextBuffer::open(path))));
            self.set_selected_buffer(self.buffers.len() - 1);
        }

        self.buffers.len() - 1
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
    pub fn render_bufferline(&self, buffer: &mut Buffer, theme: &Theme) {
        let mut current_char_offset = 0;

        for (i, short_path) in self.buffer_paths.iter().enumerate() {
            // Format the title with padding
            let title = format!(" {} ", short_path);
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
    pub fn update_paths(&mut self) {
        let paths = self.buffers.iter().map(|b| b.read().unwrap().path.clone());
        let unique_paths = get_unique_paths(paths, self.buffers.len());
        self.buffer_paths = unique_paths
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

/// System used to render the bufferline (tab bar) to the `BufferlineChunk`.
///
/// This system retrieves the `Buffers` and `Theme` resources and delegates
/// the actual rendering to the `Buffers::render_bufferline` method.
///
/// # Arguments
///
/// * `chunk`: `Chunk<BufferlineChunk>` providing mutable access to the bufferline's drawing buffer.
/// * `buffers`: `Res<Buffers>` for information about open buffers and their paths.
/// * `theme`: `Res<Theme>` for styling the bufferline.
pub async fn render_bufferline(
    chunk: Chunk<BufferlineChunk>,
    buffers: Res<Buffers>,
    theme: Res<Theme>,
) {
    let chunk = &mut chunk.get().unwrap();
    let buffers = buffers.get();
    let theme = theme.get();

    buffers.render_bufferline(chunk, &theme);
}

/// System that updates the horizontal scroll position of the bufferline.
///
/// This system ensures that the currently selected buffer's tab is always
/// visible within the bufferline display area, adjusting `tab_scroll` as needed.
///
/// # Arguments
///
/// * `buffers`: `ResMut<Buffers>` for mutable access to the bufferline scroll state.
/// * `window`: `Res<WindowState>` to get the current window width.
pub async fn update_bufferline_scroll(buffers: ResMut<Buffers>, window: Res<WindowState>) {
    let mut buffers = buffers.get();
    let window = window.get();

    if buffers.buffers.is_empty() {
        buffers.tab_scroll = 0;
        return;
    }

    // Calculate width of each tab (path + padding)
    let tab_widths: Vec<usize> = buffers.buffer_paths.iter().map(|p| p.len() + 6).collect();

    // Calculate starting character offset for each tab
    let tab_starts: Vec<usize> = tab_widths
        .iter()
        .scan(0, |acc, &w| {
            let start = *acc;
            *acc += w;
            Some(start)
        })
        .collect();

    let selected_idx = buffers.selected_buffer;
    let selected_tab_start = tab_starts[selected_idx];
    let selected_tab_end = selected_tab_start + tab_widths[selected_idx];

    let view_width = window.size().x as usize;
    let view_start = buffers.tab_scroll;
    let view_end = view_start + view_width;

    // Adjust scroll if the selected tab extends beyond the right edge
    if selected_tab_end > view_end {
        buffers.tab_scroll = selected_tab_end.saturating_sub(view_width);
    }

    // Adjust scroll if the selected tab starts before the left edge
    if selected_tab_start < view_start {
        buffers.tab_scroll = selected_tab_start;
    }

    // Ensure tab_scroll doesn't allow scrolling past the total content width
    let total_width: usize = tab_widths.iter().sum();
    if total_width < view_width {
        // If all tabs fit, reset scroll to 0
        buffers.tab_scroll = 0;
    } else {
        // Otherwise, clamp scroll to prevent empty space on the right
        buffers.tab_scroll = buffers
            .tab_scroll
            .min(total_width.saturating_sub(view_width));
    }
}

/// System that updates the active buffer's state, including its content,
/// and handles horizontal and vertical scrolling to keep the primary cursor in view.
///
/// This system is crucial for ensuring the displayed buffer content is up-to-date
/// and the user's cursor remains visible as they navigate and edit.
///
/// # Arguments
///
/// * `window`: `Res<WindowState>` to get the current window dimensions.
/// * `buffers`: `ResMut<Buffers>` for mutable access to the active `TextBuffer`.
pub async fn update_buffer(window: Res<WindowState>, buffers: ResMut<Buffers>) {
    let window = window.get();
    let mut buffers = buffers.get();

    // Re-calculate unique paths for all buffers
    buffers.update_paths();

    // Determine the visible viewport dimensions for the text buffer (excluding UI elements)
    let viewport_height = window.size().y.saturating_sub(3); // Example: 1 for bufferline, 1 for cmdline, 1 for statusline
    let viewport_width = window.size().x.saturating_sub(7); // Example: some padding

    let buffer = buffers.cur_buffer();
    let mut buffer = buffer.write().unwrap(); // Acquire write lock for the current buffer

    // Update the buffer's internal state (e.g., syntax highlighting edits)
    buffer.update();

    // Get primary cursor's byte index
    let primary_cursor_byte = buffer.primary_cursor().get_cursor_byte();

    // Calculate current row and column based on the cursor byte index
    let current_row = buffer
        .rope
        .byte_to_line_idx(primary_cursor_byte, LineType::LF_CR);
    let line_start_byte_idx = buffer.rope.line_to_byte_idx(current_row, LineType::LF_CR);
    let current_col = buffer
        .rope
        .byte_to_char_idx(primary_cursor_byte)
        .saturating_sub(buffer.rope.byte_to_char_idx(line_start_byte_idx));

    // Vertical scrolling: Adjust `buffer.scroll` to keep `current_row` visible
    if current_row < buffer.scroll {
        buffer.scroll = current_row;
    }
    if current_row >= buffer.scroll + viewport_height as usize {
        buffer.scroll = current_row.saturating_sub(viewport_height as usize) + 1;
    }

    // Horizontal scrolling: Adjust `buffer.h_scroll` to keep `current_col` visible
    if current_col < buffer.h_scroll {
        buffer.h_scroll = current_col;
    }
    if current_col >= buffer.h_scroll + viewport_width as usize {
        buffer.h_scroll = current_col.saturating_sub(viewport_width as usize) + 1;
    }
}
