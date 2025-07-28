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
}

impl Render for Buffers {
    fn render(&self, mut loc: Vec2, buffer: &mut ascii_forge::prelude::Buffer) -> Vec2 {
        let initial_loc = loc;
        for (i, buf) in self.buffers.iter().enumerate() {
            // Render Filename
            let mut style = ContentStyle::new();
            if self.selected_buffer == i {
                style = style.bold();
            }
            let title_width = buf.path.len();
            render!(buffer, loc => ["   ", StyledContent::new(style, buf.path.as_str()), "  "]);
            loc.x += title_width as u16 + 8;
        }

        loc = initial_loc;
        render!(buffer, loc => [ self.buffers[self.selected_buffer] ])
    }
}

pub fn render_buffers(mut window: ResMut<Window>, buffers: Res<Buffers>) {
    render!(window, (0, 0) => [ buffers ]);
}
