use std::sync::Arc;

use crate::{CloseEvent, EVENT_BUS, KerbinBuffer, Theme, get_canonical_path_with_non_existent};

use super::TextBuffer;
use kerbin_macros::State;
use kerbin_state_machine::storage::*;
use ratatui::buffer::Buffer;
use tokio::sync::{
    OwnedRwLockMappedWriteGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock,
};

/// Stores all text buffers managed by the editor
#[derive(Default, State)]
pub struct Buffers {
    /// The index of the currently selected buffer in the `buffers` vector
    pub selected_buffer: usize,

    /// The horizontal scroll offset of the tab-bar (bufferline) in characters
    pub tab_scroll: usize,

    /// The internal storage of `KerbinBuffer` instances
    pub buffers: Vec<Arc<RwLock<dyn KerbinBuffer>>>,

    /// The list of unique, shortened paths corresponding to each buffer
    pub buffer_paths: Vec<String>,
}

impl Buffers {
    /// Returns a read lock to the currently selected buffer
    pub async fn cur_buffer(&self) -> OwnedRwLockReadGuard<dyn KerbinBuffer> {
        self.buffers[self.selected_buffer]
            .clone()
            .read_owned()
            .await
    }

    /// Returns a write lock to the currently selected buffer
    pub async fn cur_buffer_mut(&mut self) -> OwnedRwLockWriteGuard<dyn KerbinBuffer> {
        self.buffers[self.selected_buffer]
            .clone()
            .write_owned()
            .await
    }

    /// Returns a typed read lock to the current buffer, downcast to `T`.
    /// Returns `None` if the current buffer is not of type `T`.
    pub async fn cur_buffer_as<T: 'static>(
        &self,
    ) -> Option<OwnedRwLockReadGuard<dyn KerbinBuffer, T>> {
        let guard = self.cur_buffer().await;
        OwnedRwLockReadGuard::try_map(guard, |buf| buf.as_any().downcast_ref::<T>()).ok()
    }

    /// Returns a typed write lock to the current buffer, downcast to `T`.
    /// Returns `None` if the current buffer is not of type `T`.
    pub async fn cur_buffer_as_mut<T: 'static>(
        &mut self,
    ) -> Option<OwnedRwLockMappedWriteGuard<dyn KerbinBuffer, T>> {
        let guard = self.cur_buffer_mut().await;
        OwnedRwLockWriteGuard::try_map(guard, |buf| buf.as_any_mut().downcast_mut::<T>()).ok()
    }

    /// Returns a read lock for the buffer at `path`, if open
    pub async fn get_path(&self, path: &str) -> Option<OwnedRwLockReadGuard<dyn KerbinBuffer>> {
        for buf in &self.buffers {
            if buf.read().await.title() == path {
                return Some(buf.clone().read_owned().await);
            }
        }

        None
    }

    /// Returns a write lock for the buffer at `path`, if open
    pub async fn get_mut_path(
        &mut self,
        path: &str,
    ) -> Option<OwnedRwLockWriteGuard<dyn KerbinBuffer>> {
        for buf in &self.buffers {
            if buf.read().await.title() == path {
                return Some(buf.clone().write_owned().await);
            }
        }

        None
    }

    /// Changes the selected buffer by a given signed distance
    pub fn change_buffer(&mut self, dist: isize) {
        self.selected_buffer = self
            .selected_buffer
            .saturating_add_signed(dist)
            .clamp(0, self.buffers.len() - 1);
    }

    /// Sets the selected buffer to a specific index
    pub fn set_selected_buffer(&mut self, id: usize) {
        self.selected_buffer = id.clamp(0, self.buffers.len() - 1);
    }

    /// Closes the buffer at the given index
    pub async fn close_buffer(&mut self, idx: usize) {
        let buf = self.buffers.remove(idx);

        EVENT_BUS.emit(CloseEvent { buffer: buf }).await;

        if self.buffers.is_empty() {
            self.buffers.push(
                Arc::new(RwLock::new(TextBuffer::scratch())) as Arc<RwLock<dyn KerbinBuffer>>,
            );
        }

        self.change_buffer(0); // Adjust selected_buffer to remain valid
    }

    /// Opens a buffer with the given file path
    pub async fn open(&mut self, path: String, default_tab_unit: usize) -> std::io::Result<usize> {
        let check_path = get_canonical_path_with_non_existent(&path)
            .to_str()
            .unwrap()
            .to_string();

        let mut found_buffer_id: Option<usize> = None;

        for (i, buffer_arc) in self.buffers.iter().enumerate() {
            let buffer_read = buffer_arc.read().await;

            if buffer_read.title() == check_path {
                found_buffer_id = Some(i);
                break;
            }
        }

        if let Some(buffer_id) = found_buffer_id {
            self.set_selected_buffer(buffer_id);
            Ok(buffer_id)
        } else {
            let new_buffer =
                Arc::new(RwLock::new(TextBuffer::open(path, default_tab_unit)?))
                    as Arc<RwLock<dyn KerbinBuffer>>;
            self.buffers.push(new_buffer);
            let new_buffer_id = self.buffers.len() - 1;
            self.set_selected_buffer(new_buffer_id);

            Ok(new_buffer_id)
        }
    }

    /// Inserts a `TextBuffer` safely into the buffers, deduplicating by title
    pub async fn push_new(&mut self, buffer: TextBuffer) -> usize {
        let mut found_buffer_id: Option<usize> = None;

        for (i, buffer_arc) in self.buffers.iter().enumerate() {
            let buffer_read = buffer_arc.read().await;

            if buffer_read.title() == buffer.path {
                found_buffer_id = Some(i);
                break;
            }
        }

        if let Some(buffer_id) = found_buffer_id {
            self.close_buffer(buffer_id).await;
        }

        let new_buffer =
            Arc::new(RwLock::new(buffer)) as Arc<RwLock<dyn KerbinBuffer>>;
        self.buffers.push(new_buffer);
        let new_buffer_id = self.buffers.len() - 1;
        self.set_selected_buffer(new_buffer_id);

        new_buffer_id
    }

    /// Inserts any `KerbinBuffer` implementor into the buffer list
    pub async fn push_buffer<T: KerbinBuffer>(&mut self, buffer: T) -> usize {
        let new_buffer: Arc<RwLock<dyn KerbinBuffer>> = Arc::new(RwLock::new(buffer));
        self.buffers.push(new_buffer);
        let new_buffer_id = self.buffers.len() - 1;
        self.set_selected_buffer(new_buffer_id);
        new_buffer_id
    }

    /// Renders the bufferline (tab bar) into the provided `Buffer`.
    /// Legacy method — shows all global buffers with `selected_buffer` highlighted.
    pub async fn render_bufferline(&self, buffer: &mut Buffer, theme: &Theme) {
        let indices: Vec<usize> = (0..self.buffers.len()).collect();
        self.render_bufferline_pane(buffer, theme, &indices, self.selected_buffer, self.tab_scroll)
            .await;
    }

    /// Renders a bufferline for one specific pane.
    ///
    /// - `displayed_global_indices`: which global buffer indices to show as tabs.
    /// - `active_display_idx`: position within `displayed_global_indices` that is highlighted.
    /// - `tab_scroll`: horizontal scroll offset in characters.
    pub async fn render_bufferline_pane(
        &self,
        buffer: &mut Buffer,
        theme: &Theme,
        displayed_global_indices: &[usize],
        active_display_idx: usize,
        tab_scroll: usize,
    ) {
        let mut current_char_offset = 0;

        for (display_i, &global_i) in displayed_global_indices.iter().enumerate() {
            let short_path = match self.buffer_paths.get(global_i) {
                Some(p) => p,
                None => continue,
            };
            let dirty = match self.buffers.get(global_i) {
                Some(b) => b.read().await.is_dirty(),
                None => continue,
            };

            let title = format!("   {} {} ", short_path, if dirty { "*" } else { " " });
            let title_width = title.chars().count();

            let visible_range_start = tab_scroll;
            let visible_range_end = tab_scroll + buffer.area.width as usize;
            let tab_range_start = current_char_offset;
            let tab_range_end = current_char_offset + title_width;

            let style = if display_i == active_display_idx {
                theme.get_fallback_default(["ui.bufferline.selected", "ui.bufferline", "ui.text"])
            } else {
                theme.get_fallback_default(["ui.bufferline", "ui.text"])
            };

            let overlap_start = visible_range_start.max(tab_range_start);
            let overlap_end = visible_range_end.min(tab_range_end);

            if overlap_start < overlap_end {
                let slice_start = overlap_start - tab_range_start;
                let slice_len = overlap_end - overlap_start;
                let visible_part: String =
                    title.chars().skip(slice_start).take(slice_len).collect();

                let render_x = (overlap_start - tab_scroll) as u16;
                buffer.set_string(
                    buffer.area.x + render_x,
                    buffer.area.y,
                    &visible_part,
                    style,
                );
            }

            current_char_offset += title_width;
        }
    }

    /// Updates the unique, shortened paths for all currently open buffers
    pub async fn update_paths(&mut self) {
        let mut paths: Vec<String> = Vec::with_capacity(self.buffers.len());

        for buffer_arc in self.buffers.iter() {
            let buffer_read = buffer_arc.read().await;
            paths.push(buffer_read.title());
        }

        let unique_paths = get_unique_paths(paths.into_iter(), self.buffers.len());
        self.buffer_paths = unique_paths;
    }

    /// Returns the unique (shortened) path of the buffer at the given index
    pub fn unique_path_of(&self, idx: usize) -> Option<String> {
        self.buffer_paths.get(idx).cloned()
    }
}

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
                .filter(|(j, _)| *j != i)
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

            if is_unique || depth >= path_components[i].len() {
                truncated_paths[i] = truncated;
                break;
            }
            depth += 1;
        }
    }

    truncated_paths
}
