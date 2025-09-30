// Ideas
// - Store Byte based scroll (what line should the renderer calculate from)
// - Store sub, row based scroll (what offset of that line should be shown)
// - Update the byte based scroll from the sub scroll each time the sub scroll is moved
// - Each line will need to be rendered fully, but this should be fine

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

/// Default renderer for a `TextBuffer`.
///
/// This function renders:
/// - The rope text itself
/// - Any extmarks in the visible viewport
///
/// Cursor positions and selections are *not* hardcoded here – they are provided
/// each frame as [`Extmark`]s by the [`render_cursors_and_selections`] system.
pub async fn render_buffer_default(chunk: Chunk<BufferChunk>, buffers: Res<Buffers>) {
    let Some(mut chunk) = chunk.get() else { return };
    get!(buffers);

    let buf = buffers.cur_buffer();
    let buf = buf.read().unwrap();

    let mut pos = vec2(0, 0);

    // Skip the scrolled lines
    for line in buf.renderer.lines.iter().skip(buf.renderer.visual_scroll) {
        // Stop rendering if we've filled the viewport
        if pos.y >= chunk.size().y {
            break;
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
}
