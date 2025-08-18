use std::sync::{Arc, RwLock};

use crate::{GrammarManager, Theme, state::State};

use super::TextBuffer;
use ascii_forge::prelude::*;

#[derive(Default)]
pub struct Buffers {
    pub selected_buffer: usize,
    pub tab_scroll: usize,

    pub buffers: Vec<Arc<RwLock<TextBuffer>>>,
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

    pub fn close_current_buffer(&mut self) {
        self.buffers.remove(self.selected_buffer);
        if self.buffers.is_empty() {
            self.buffers
                .push(Arc::new(RwLock::new(TextBuffer::scratch())));
        }

        self.change_buffer(0);
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
        if let Some(buffer_id) = self.buffers.iter().enumerate().find_map(|(i, x)| {
            if x.read().unwrap().path == path {
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

    pub fn render(
        &self,
        loc: Vec2,
        buffer: &mut ascii_forge::prelude::Buffer,
        theme: &Theme,
    ) -> Vec2 {
        let mut inner_buffer = Buffer::new(buffer.size() - vec2(0, 3));
        let initial_loc = loc;
        let mut current_char_offset = 0;

        let paths: Vec<String> = self
            .buffers
            .iter()
            .map(|b| b.read().unwrap().path.clone())
            .collect();
        let unique_paths = get_unique_paths(paths);

        for (i, _buf) in self.buffers.iter().enumerate() {
            let title = format!("   {}   ", unique_paths[i]);
            let title_width = title.chars().count();

            let visible_range_start = self.tab_scroll;
            let visible_range_end = self.tab_scroll + buffer.size().x as usize;
            let tab_range_start = current_char_offset;
            let tab_range_end = current_char_offset + title_width;

            let overlap_start = visible_range_start.max(tab_range_start);
            let overlap_end = visible_range_end.min(tab_range_end);

            if overlap_start < overlap_end {
                let slice_start = overlap_start - tab_range_start;
                let slice_len = overlap_end - overlap_start;
                let visible_part: String =
                    title.chars().skip(slice_start).take(slice_len).collect();

                let render_x = (overlap_start - self.tab_scroll) as u16;
                render!(buffer, initial_loc + vec2(render_x, 0) => [visible_part]);
            }

            current_char_offset += title_width;
        }

        let mut content_loc = initial_loc;
        content_loc.y += 1;
        self.buffers[self.selected_buffer].read().unwrap().render(
            vec2(0, 0),
            &mut inner_buffer,
            theme,
        );
        render!(buffer, content_loc => [ inner_buffer ])
    }
}

fn get_unique_paths(paths: Vec<String>) -> Vec<String> {
    if paths.is_empty() {
        return vec![];
    }

    let path_components: Vec<Vec<&str>> = paths.iter().map(|p| p.split('/').collect()).collect();

    let mut truncated_paths: Vec<String> = paths.iter().map(|_| String::new()).collect();

    for i in 0..paths.len() {
        let mut depth = 1;
        loop {
            let truncated_parts: Vec<&str> = path_components[i]
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
                        .collect::<Vec<&str>>()
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

pub fn render_buffers(state: Arc<State>) {
    let theme = state.theme.read().unwrap();
    let mut window = state.window.write().unwrap();
    let buffers = state.buffers.read().unwrap();

    //let mut top_bar = Buffer::new(vec2(window.size().x, 1));
    //top_bar.style_line(0, |_| style);
    //render!(window.buffer_mut(), vec2(0, 0) => [top_bar]);
    buffers.render(vec2(0, 0), window.buffer_mut(), &theme);
}

pub fn update_bufferline_scroll(state: Arc<State>) {
    let mut buffers = state.buffers.write().unwrap();
    let window = state.window.read().unwrap();

    if buffers.buffers.is_empty() {
        buffers.tab_scroll = 0;
        return;
    }

    let paths: Vec<String> = buffers
        .buffers
        .iter()
        .map(|b| b.read().unwrap().path.clone())
        .collect();
    let unique_paths = get_unique_paths(paths);
    let tab_widths: Vec<usize> = unique_paths.iter().map(|p| p.len() + 6).collect();

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

pub fn update_buffer(state: Arc<State>) {
    let window = state.window.read().unwrap();
    let buffers = state.buffers.read().unwrap();

    let viewport_height = window.size().y.saturating_sub(3);
    let viewport_width = window.size().x.saturating_sub(7);
    let buffer = buffers.cur_buffer();
    let mut buffer = buffer.write().unwrap();

    if buffer.row < buffer.scroll {
        buffer.scroll = buffer.row;
    }

    if buffer.row >= buffer.scroll + viewport_height as usize {
        buffer.scroll = buffer.row - viewport_height as usize + 1;
    }

    if buffer.col < buffer.h_scroll {
        buffer.h_scroll = buffer.col;
    }

    if buffer.col >= buffer.h_scroll + viewport_width as usize {
        buffer.h_scroll = buffer.col - viewport_width as usize + 1;
    }
}
