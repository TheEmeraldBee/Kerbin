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

    theme: Res<Theme>,
    core_config: Res<CoreConfig>,
) {
    let Some(mut chunk) = chunk.get().await else {
        return;
    };

    get!(buffers, theme, core_config);

    let buf = buffers.cur_buffer().await;

    let area = chunk.area();

    // Render text buffer
    let tab_style = theme.get_fallback_default(["ui.text.tabs", "ui.text"]);
    let cursor_on_tab_style = theme.get_fallback_default(["ui.selection"]);
    let mut cursor_state = CursorRenderState::default();
    TextBufferWidget::new(&buf)
        .with_vertical_scroll(buf.renderer.byte_scroll)
        .with_horizontal_scroll(buf.renderer.h_scroll)
        .with_tab_display_unit(core_config.tab_display_unit.clone())
        .with_tab_style(tab_style)
        .with_cursor_on_tab_style(cursor_on_tab_style)
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
        GutterWidget::new(buf.renderer.byte_scroll, buf.len_lines(), &theme)
            .render(gutter_area, &mut gutter);
    }
}
