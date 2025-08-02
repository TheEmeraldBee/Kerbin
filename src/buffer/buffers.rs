use std::{cell::RefCell, rc::Rc};

use crate::{GrammarManager, Theme};

use super::TextBuffer;
use ascii_forge::prelude::*;
use derive_more::*;
use rune::Any;
use stategine::prelude::*;

#[derive(Deref, DerefMut, Default, Any)]
pub struct Buffers {
    pub selected_buffer: usize,

    #[deref]
    #[deref_mut]
    pub buffers: Vec<Rc<RefCell<TextBuffer>>>,
}

impl Buffers {
    pub fn cur_buffer(&self) -> Rc<RefCell<TextBuffer>> {
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
                .push(Rc::new(RefCell::new(TextBuffer::scratch())));
        }

        self.change_buffer(0);
    }

    pub fn close_buffer(&mut self, idx: usize) {
        self.buffers.remove(idx);
        if self.buffers.is_empty() {
            self.buffers
                .push(Rc::new(RefCell::new(TextBuffer::scratch())));
        }

        self.change_buffer(0);
    }

    pub fn open(&mut self, path: String, grammar: &mut GrammarManager, theme: &Theme) {
        if let Some(buffer_id) = self.buffers.iter().enumerate().find_map(|(i, x)| {
            if x.borrow().path == path {
                Some(i)
            } else {
                None
            }
        }) {
            self.set_selected_buffer(buffer_id);
        } else {
            self.buffers.push(Rc::new(RefCell::new(TextBuffer::open(
                path, grammar, theme,
            ))));
            self.set_selected_buffer(self.buffers.len() - 1)
        }
    }
    fn render(
        &self,
        mut loc: Vec2,
        buffer: &mut ascii_forge::prelude::Buffer,
        theme: &Theme,
    ) -> Vec2 {
        let mut inner_buffer = Buffer::new(buffer.size() - vec2(0, 3));
        let initial_loc = loc;
        for (i, buf) in self.buffers.iter().enumerate() {
            // Render Filename
            let mut style = ContentStyle::new();
            if self.selected_buffer == i {
                style = style.bold();
            }
            let title_width = buf.borrow().path.len();
            render!(buffer, loc => ["   ", StyledContent::new(style, buf.borrow().path.as_str()), "   "]);
            loc.x += title_width as u16 + 6;
        }

        loc = initial_loc;
        loc.y += 1;
        self.buffers[self.selected_buffer]
            .borrow()
            .render(vec2(0, 0), &mut inner_buffer, theme);
        render!(buffer, loc => [ inner_buffer ])
    }
}

pub fn render_buffers(mut window: ResMut<Window>, buffers: Res<Buffers>, theme: Res<Theme>) {
    buffers.render(vec2(0, 0), window.buffer_mut(), &theme);
}

pub fn update_buffer(window: Res<Window>, buffers: Res<Buffers>) {
    let viewport_height = window.size().y.saturating_sub(3);
    let buffer = buffers.cur_buffer();
    let mut buffer = buffer.borrow_mut();

    // If cursor is above the visible area, scroll up to bring it into view.
    if buffer.cursor_pos.y < buffer.scroll as u16 {
        buffer.scroll = buffer.cursor_pos.y as usize;
    }

    // If cursor is below the visible area, scroll down to bring it into view.
    if buffer.cursor_pos.y >= buffer.scroll as u16 + viewport_height {
        buffer.scroll = buffer.cursor_pos.y as usize - viewport_height as usize + 1;
    }
}
