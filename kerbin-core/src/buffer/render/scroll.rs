use crate::*;
use unicode_segmentation::UnicodeSegmentation;

/// Converts a display column to the byte offset within `line_text` at which
/// that column begins. Handles tabs (expanded to `tab_display_width` cells),
/// emoji variation selectors, and wide Unicode.
pub fn display_col_to_byte_offset(
    line_text: &str,
    target_display_col: usize,
    tab_display_width: usize,
) -> usize {
    let mut byte_offset = 0usize;
    let mut current_width = 0usize;
    for g in line_text.graphemes(true) {
        if g == "\n" || g == "\r\n" || g == "\r" {
            break;
        }
        if current_width >= target_display_col {
            break;
        }
        let w = if g == "\t" {
            tab_display_width
        } else {
            grapheme_display_width(g)
        };
        current_width += w;
        byte_offset += g.len();
    }
    byte_offset
}

pub async fn update_buffer_horizontal_scroll(chunk: Chunk<BufferChunk>, buffers: ResMut<Buffers>) {
    let Some(chunk) = chunk.get().await else {
        return;
    };
    let viewport_width = chunk.area().width as usize;

    get!(mut buffers);

    let Some(mut buf) = buffers.cur_buffer_as_mut::<TextBuffer>().await else {
        return;
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
    let cursor_line_idx = buf.byte_to_line_clamped(cursor_byte);
    let line_start_byte = buf.line_to_byte_clamped(cursor_line_idx);

    let line_text = buf
        .slice_to_string(line_start_byte, cursor_byte)
        .unwrap_or_default();
    let cursor_col: usize = line_text.graphemes(true).map(grapheme_display_width).sum();

    const H_SCROLL_PADDING: usize = 5;

    let cursor_viewport_col = cursor_col.saturating_sub(buf.renderer.h_scroll);

    if cursor_viewport_col >= viewport_width.saturating_sub(H_SCROLL_PADDING) {
        let target_col = viewport_width.saturating_sub(H_SCROLL_PADDING + 1);
        let needed_scroll = cursor_viewport_col - target_col;
        buf.renderer.h_scroll += needed_scroll;
    }

    if cursor_viewport_col < H_SCROLL_PADDING {
        let needed_scroll = H_SCROLL_PADDING - cursor_viewport_col;
        if buf.renderer.h_scroll >= needed_scroll {
            buf.renderer.h_scroll -= needed_scroll;
        } else {
            buf.renderer.h_scroll = 0;
        }
    }

    if cursor_col < buf.renderer.h_scroll {
        buf.renderer.h_scroll = cursor_col.saturating_sub(H_SCROLL_PADDING);
    }
}

pub async fn update_buffer_vertical_scroll(chunk: Chunk<BufferChunk>, buffers: ResMut<Buffers>) {
    let Some(chunk) = chunk.get().await else {
        return;
    };
    let viewport_height = chunk.area().height as usize;

    get!(mut buffers);

    let Some(mut buf) = buffers.cur_buffer_as_mut::<TextBuffer>().await else {
        return;
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
    let cursor_line_idx = buf.byte_to_line_clamped(cursor_byte);

    const SCROLL_PADDING: usize = 3;

    if cursor_line_idx < buf.renderer.byte_scroll {
        buf.renderer.byte_scroll = cursor_line_idx.saturating_sub(SCROLL_PADDING);
        buf.renderer.visual_scroll = 0;
        return;
    }

    let cursor_viewport_position = cursor_line_idx - buf.renderer.byte_scroll;

    if cursor_viewport_position >= viewport_height.saturating_sub(SCROLL_PADDING) {
        buf.renderer.byte_scroll =
            cursor_line_idx.saturating_sub(viewport_height.saturating_sub(SCROLL_PADDING + 1));
    }

    buf.renderer.visual_scroll = 0;

    let max_byte_scroll = buf.len_lines().saturating_sub(1);
    buf.renderer.byte_scroll = buf.renderer.byte_scroll.min(max_byte_scroll);
}
