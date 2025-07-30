use crate::{GrammarManager, HighlightConfiguration};

use super::TextBuffer;
use ascii_forge::prelude::*;
use derive_more::*;
use stategine::prelude::*;

#[derive(Deref, DerefMut, Default)]
pub struct Buffers {
    pub selected_buffer: usize,

    #[deref]
    #[deref_mut]
    pub buffers: Vec<TextBuffer>,
}

impl Buffers {
    pub fn cur_buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffers[self.selected_buffer]
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
            self.buffers.push(TextBuffer::scratch());
        }

        self.change_buffer(0);
    }

    pub fn close_buffer(&mut self, idx: usize) {
        self.buffers.remove(idx);
        if self.buffers.is_empty() {
            self.buffers.push(TextBuffer::scratch());
        }

        self.change_buffer(0);
    }

    pub fn open(
        &mut self,
        path: String,
        grammar: &mut GrammarManager,
        hl_conf: &HighlightConfiguration,
    ) {
        if let Some(buffer_id) = self
            .buffers
            .iter()
            .enumerate()
            .find_map(|(i, x)| if x.path == path { Some(i) } else { None })
        {
            self.set_selected_buffer(buffer_id);
        } else {
            self.buffers.push(TextBuffer::open(path, grammar, hl_conf));
            self.set_selected_buffer(self.buffers.len() - 1)
        }
    }
}

impl Render for Buffers {
    fn render(&self, mut loc: Vec2, buffer: &mut ascii_forge::prelude::Buffer) -> Vec2 {
        let mut inner_buffer = Buffer::new(buffer.size() - vec2(0, 3));
        let initial_loc = loc;
        for (i, buf) in self.buffers.iter().enumerate() {
            // Render Filename
            let mut style = ContentStyle::new();
            if self.selected_buffer == i {
                style = style.bold();
            }
            let title_width = buf.path.len();
            render!(buffer, loc => ["   ", StyledContent::new(style, buf.path.as_str()), "   "]);
            loc.x += title_width as u16 + 6;
        }

        loc = initial_loc;
        loc.y += 1;
        render!(inner_buffer, vec2(0, 0) => [self.buffers[self.selected_buffer]]);
        render!(buffer, loc => [ inner_buffer ])
    }
}

pub fn render_buffers(mut window: ResMut<Window>, buffers: Res<Buffers>) {
    render!(window, (0, 0) => [ buffers ]);
}

pub fn update_buffer(window: Res<Window>, mut buffers: ResMut<Buffers>) {
    let viewport_height = window.size().y.saturating_sub(3);
    let buffer = buffers.cur_buffer_mut();

    // If cursor is above the visible area, scroll up to bring it into view.
    if buffer.cursor_pos.y < buffer.scroll as u16 {
        buffer.scroll = buffer.cursor_pos.y as usize;
    }

    // If cursor is below the visible area, scroll down to bring it into view.
    if buffer.cursor_pos.y >= buffer.scroll as u16 + viewport_height {
        buffer.scroll = buffer.cursor_pos.y as usize - viewport_height as usize + 1;
    }
}
