use std::sync::{Arc, RwLock};

use crate::{
    BufferChunk, BufferlineChunk, Chunk, GrammarManager, ModeStack, Theme, WindowState,
    get_canonical_path_with_non_existent,
};

use super::TextBuffer;
use ascii_forge::prelude::*;
use kerbin_macros::State;
use kerbin_state_machine::storage::*;
use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};
use ropey::LineType;

#[derive(Default, State)]
pub struct Buffers {
    pub selected_buffer: usize,
    pub tab_scroll: usize,

    pub buffers: Vec<Arc<RwLock<TextBuffer>>>,
    pub buffer_paths: Vec<String>,
}

impl Buffers {
    pub fn cur_buffer(&self) -> Arc<RwLock<TextBuffer>> {
        self.buffers[self.selected_buffer].clone()
    }

    pub fn change_buffer(&mut self, dist: isize) {
        self.selected_buffer = self
            .selected_buffer
            .saturating_add_signed(dist)
            .clamp(0, self.buffers.len() - 1);
    }

    pub fn set_selected_buffer(&mut self, id: usize) {
        self.selected_buffer = id.clamp(0, self.buffers.len() - 1);
    }

    pub fn close_buffer(&mut self, idx: usize) {
        self.buffers.remove(idx);
        if self.buffers.is_empty() {
            self.buffers
                .push(Arc::new(RwLock::new(TextBuffer::scratch())));
        }

        self.change_buffer(0);
    }

    pub fn open(
        &mut self,
        path: String,
        grammar_manager: &mut GrammarManager,
        theme: &Theme,
    ) -> usize {
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
            self.buffers.push(Arc::new(RwLock::new(TextBuffer::open(
                path,
                grammar_manager,
                theme,
            ))));
            self.set_selected_buffer(self.buffers.len() - 1);
        }

        self.buffers.len() - 1
    }

    pub fn render_bufferline(&self, buffer: &mut Buffer, theme: &Theme) {
        let mut current_char_offset = 0;

        for (i, short_path) in self.buffer_paths.iter().enumerate() {
            let title = format!("   {}   ", short_path);
            let title_width = title.chars().count();

            let visible_range_start = self.tab_scroll;
            let visible_range_end = self.tab_scroll + buffer.size().x as usize;
            let tab_range_start = current_char_offset;
            let tab_range_end = current_char_offset + title_width;

            let style = if i == self.selected_buffer {
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

                let render_x = (overlap_start - self.tab_scroll) as u16;
                render!(buffer, vec2(render_x, 0) => [ StyledContent::new(style, visible_part) ]);
            }

            current_char_offset += title_width;
        }
    }

    pub fn render(&mut self, buffer: &mut Buffer, theme: &Theme, modes: Vec<char>) {
        self.update_paths();

        self.buffers[self.selected_buffer]
            .read()
            .unwrap()
            .render(buffer, theme, modes);
    }

    pub fn update_paths(&mut self) {
        let paths = self.buffers.iter().map(|b| b.read().unwrap().path.clone());
        let unique_paths = get_unique_paths(paths, self.buffers.len());
        self.buffer_paths = unique_paths
    }

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
            let truncated_parts: Vec<String> = path_components[i]
                .iter()
                .rev()
                .take(depth)
                .rev()
                .cloned()
                .collect();
            let truncated = truncated_parts.join("/");

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

pub async fn render_buffers(
    chunk: Chunk<BufferChunk>,
    buffers: ResMut<Buffers>,
    theme: Res<Theme>,
    modes: Res<ModeStack>,
) {
    let theme = theme.get();
    let mut buffers = buffers.get();
    let modes = modes.get();
    let mut chunk = &mut chunk.get().unwrap();

    buffers.render(&mut chunk, &theme, modes.0.clone());
}

pub async fn update_bufferline_scroll(buffers: ResMut<Buffers>, window: Res<WindowState>) {
    let mut buffers = buffers.get();
    let window = window.get();

    if buffers.buffers.is_empty() {
        buffers.tab_scroll = 0;
        return;
    }

    let tab_widths: Vec<usize> = buffers.buffer_paths.iter().map(|p| p.len() + 6).collect();

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

    if selected_tab_end > view_end {
        buffers.tab_scroll = selected_tab_end.saturating_sub(view_width);
    }

    if selected_tab_start < view_start {
        buffers.tab_scroll = selected_tab_start;
    }

    let total_width: usize = tab_widths.iter().sum();
    if total_width < view_width {
        buffers.tab_scroll = 0;
    } else {
        buffers.tab_scroll = buffers
            .tab_scroll
            .min(total_width.saturating_sub(view_width));
    }
}

pub async fn update_buffer(window: Res<WindowState>, buffers: Res<Buffers>, theme: Res<Theme>) {
    let window = window.get();
    let buffers = buffers.get();
    let theme = theme.get();

    let viewport_height = window.size().y.saturating_sub(3);
    let viewport_width = window.size().x.saturating_sub(7);
    let buffer = buffers.cur_buffer();
    let mut buffer = buffer.write().unwrap();

    buffer.update(&theme);

    // Calculate current row and column based on the cursor byte index
    let current_row = buffer
        .rope
        .byte_to_line_idx(buffer.primary_cursor().get_cursor_byte(), LineType::LF_CR);
    let line_start_byte_idx = buffer.rope.line_to_byte_idx(current_row, LineType::LF_CR);
    let current_col = buffer
        .rope
        .byte_to_char_idx(buffer.primary_cursor().get_cursor_byte())
        - buffer.rope.byte_to_char_idx(line_start_byte_idx);

    // Vertical scrolling
    if current_row < buffer.scroll {
        buffer.scroll = current_row;
    }

    if current_row >= buffer.scroll + viewport_height as usize {
        buffer.scroll = current_row - viewport_height as usize + 1;
    }

    // Horizontal scrolling
    if current_col < buffer.h_scroll {
        buffer.h_scroll = current_col;
    }

    if current_col >= buffer.h_scroll + viewport_width as usize {
        buffer.h_scroll = current_col - viewport_width as usize + 1;
    }
}
