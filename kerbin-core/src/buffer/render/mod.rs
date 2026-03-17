pub mod renderer;
pub use renderer::*;

pub mod scroll;
pub use scroll::*;

pub mod widget;
pub use widget::*;

pub mod gutter;
pub use gutter::*;

use crate::*;
use ratatui::prelude::*;

pub async fn render_buffer_default(
    gutter_chunk: Chunk<BufferGutterChunk>,
    chunk: Chunk<BufferChunk>,
    buffers: Res<Buffers>,
) {
    let Some(mut chunk) = chunk.get().await else {
        return;
    };

    get!(buffers);

    let buf = buffers.cur_buffer().await;

    let area = chunk.area();

    // Render text buffer
    let mut cursor_state = CursorRenderState::default();
    TextBufferWidget::new(&buf)
        .with_vertical_scroll(buf.renderer.byte_scroll)
        .with_horizontal_scroll(buf.renderer.h_scroll)
        .render(area, &mut chunk, &mut cursor_state);

    // Store cursor into chunk if found
    if let Some((cx, cy, shape)) = cursor_state.cursor {
        chunk.set_cursor(0, cx, cy, shape);
    } else {
        chunk.remove_cursor();
    }

    // Render gutter
    if let Some(mut gutter) = gutter_chunk.get().await {
        let gutter_area = gutter.area();
        GutterWidget::new(buf.renderer.byte_scroll, buf.len_lines())
            .render(gutter_area, &mut gutter);
    }
}
