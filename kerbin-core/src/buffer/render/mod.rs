pub mod renderer;
pub use renderer::*;

pub mod render_line;
pub use render_line::*;

pub mod build;
pub use build::*;

pub mod scroll;
pub use scroll::*;

use crate::*;
use ascii_forge::prelude::*;

pub async fn render_buffer_default(
    gutter_chunk: Chunk<BufferGutterChunk>,
    chunk: Chunk<BufferChunk>,
    buffers: Res<Buffers>,
) {
    let Some(mut chunk) = chunk.get().await else {
        return;
    };

    let mut gutter = gutter_chunk.get().await;
    get!(buffers);

    let buf = buffers.cur_buffer().await;

    let mut pos = vec2(0, 0);

    // Skip the scrolled lines
    for line in buf.renderer.lines.iter().skip(buf.renderer.visual_scroll) {
        // Stop rendering if we've filled the viewport
        if pos.y >= chunk.size().y {
            break;
        }

        if let Some(gutter) = &mut gutter {
            line.render_gutter(gutter, vec2(0, pos.y));
        }

        line.render(&mut chunk, pos, buf.renderer.h_scroll);

        if let Some((byte, style)) = buf.renderer.cursor
            && let Some(col) = line.byte_to_col(byte)
            && col >= buf.renderer.h_scroll
            && col < buf.renderer.h_scroll + chunk.size().x as usize
        {
            let render_col = col.saturating_sub(buf.renderer.h_scroll);
            chunk.set_cursor(0, pos + vec2(render_col as u16, 0), style);
        }

        pos.y += 1;
    }

    let mut overlay_renderer = OverlayRenderer::default();
    overlay_renderer.collect_from_lines(&buf.renderer.lines, buf.renderer.byte_scroll);
    overlay_renderer.render_all(
        &mut chunk,
        vec2(0, 0),
        buf.renderer.byte_scroll,
        buf.renderer.h_scroll,
        1,
    );
}
