use ascii_forge::window::{Buffer, ContentStyle};

pub trait BufferExtension {
    fn style_line(&mut self, y_line: u16, style_fn: impl Fn(ContentStyle) -> ContentStyle);
}

impl BufferExtension for Buffer {
    fn style_line(&mut self, y_line: u16, style_fn: impl Fn(ContentStyle) -> ContentStyle) {
        for x in 0..self.size().x {
            let Some(cell) = self.get_mut((x, y_line)) else {
                continue;
            };
            let style = cell.style_mut();
            *style = style_fn(*style);
        }
    }
}
