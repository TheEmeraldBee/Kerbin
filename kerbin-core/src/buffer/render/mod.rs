pub mod renderer;
pub use renderer::*;

pub mod scroll;
pub use scroll::*;

pub mod widget;
pub use widget::*;

pub mod gutter;
pub use gutter::*;

use crate::*;

pub fn grapheme_display_width(g: &str) -> usize {
    if g.contains('\u{FE0F}') {
        return 2;
    }
    UnicodeWidthStr::width(g)
}

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

    let ctx = RenderContext { theme: &theme, core_config: &core_config };
    let area = chunk.area();
    let buf_arc = buffers.buffers[buffers.selected_buffer].clone();
    let mut buf = buf_arc.write_owned().await;
    buf.render(area, &mut chunk, true, &ctx);

    if let Some(mut gutter) = gutter_chunk.get().await {
        let gutter_area = gutter.area();
        buf.render_gutter(gutter_area, &mut gutter, &ctx);
    }
}

/// Renders all non-focused panes into their indexed chunks.
/// The focused pane is handled by `render_buffer_default`.
pub async fn render_splits(
    chunks: Res<Chunks>,
    split: Res<SplitState>,
    buffers: Res<Buffers>,
    theme: Res<Theme>,
    core_config: Res<CoreConfig>,
) {
    get!(chunks, split, buffers, theme, core_config);

    if split.pane_count() <= 1 {
        return;
    }

    let ctx = RenderContext { theme: &theme, core_config: &core_config };
    let focused_id = split.focused_id;
    for (i, pane) in split.leaves().iter().enumerate() {
        if pane.id == focused_id {
            continue;
        }

        let buf_idx = if !split.unique_buffers {
            pane.selected_local
        } else if let Some(&idx) = pane.buffer_indices.get(pane.selected_local) {
            idx
        } else {
            continue;
        };

        if buf_idx >= buffers.buffers.len() {
            continue;
        }

        let mut buf = buffers.buffers[buf_idx].clone().write_owned().await;

        let Some(chunk_arc) = chunks.get_indexed_chunk::<BufferChunk>(i) else {
            continue;
        };
        let mut chunk = chunk_arc.write_owned().await;
        let area = chunk.area();
        buf.render(area, &mut chunk, false, &ctx);

        if let Some(gutter_arc) = chunks.get_indexed_chunk::<BufferGutterChunk>(i) {
            let mut gutter = gutter_arc.write_owned().await;
            let gutter_area = gutter.area();
            buf.render_gutter(gutter_area, &mut gutter, &ctx);
        }

        if let Some(bl_arc) = chunks.get_indexed_chunk::<BufferlineChunk>(i) {
            let mut bl = bl_arc.write_owned().await;
            let displayed_global_indices: Vec<usize> = if !split.unique_buffers {
                (0..buffers.buffers.len()).collect()
            } else {
                pane.buffer_indices.clone()
            };
            buffers
                .render_bufferline_pane(
                    &mut bl,
                    &theme,
                    &displayed_global_indices,
                    pane.selected_local,
                    pane.tab_scroll,
                )
                .await;
        }
    }
}
