use ascii_forge::prelude::*;

pub trait BufferExts {
    /// Render the buffer to another buffer, ignoring \0 characters
    /// Used internally for rendering chunks to the screen.
    fn render_non_empty(&self, loc: Vec2, buffer: &mut Buffer) -> Vec2;
}

impl BufferExts for Buffer {
    fn render_non_empty(&self, loc: Vec2, buffer: &mut Buffer) -> Vec2 {
        for x in 0..self.size().x {
            if x + loc.x >= buffer.size().x {
                break;
            }
            for y in 0..self.size().y {
                if y + loc.y >= buffer.size().y {
                    break;
                }

                let source_pos = vec2(x, y);
                let dest_pos = vec2(x + loc.x, y + loc.y);

                if let Some(cell) = self.get(source_pos) {
                    if cell.text() != "\0" {
                        buffer.set(dest_pos, cell.clone());
                    }
                }
            }
        }
        vec2(loc.x + self.size().x, loc.y + self.size().y)
    }
}
