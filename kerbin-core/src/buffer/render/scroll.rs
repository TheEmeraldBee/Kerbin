use crate::*;
use unicode_segmentation::UnicodeSegmentation;

/// Converts a display column to the byte offset based on visual positions
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

pub async fn update_buffer_horizontal_scroll(
    chunks: Res<Chunks>,
    split: Res<SplitState>,
    buffers: ResMut<Buffers>,
) {
    get!(chunks, split, mut buffers);

    let viewport_width = split
        .focused_leaf_idx()
        .and_then(|i| chunks.rect_for_indexed_chunk::<BufferChunk>(i))
        .or_else(|| chunks.rect_for_chunk(&BufferChunk::static_name()))
        .map(|r| r.width as usize)
        .unwrap_or(0);

    if viewport_width == 0 {
        return;
    }

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else {
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

pub async fn update_buffer_vertical_scroll(
    chunks: Res<Chunks>,
    split: Res<SplitState>,
    buffers: ResMut<Buffers>,
) {
    get!(chunks, split, mut buffers);

    let viewport_height = split
        .focused_leaf_idx()
        .and_then(|i| chunks.rect_for_indexed_chunk::<BufferChunk>(i))
        .or_else(|| chunks.rect_for_chunk(&BufferChunk::static_name()))
        .map(|r| r.height as usize)
        .unwrap_or(0);

    if viewport_height == 0 {
        return;
    }

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else {
        return;
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte().min(buf.len());
    let cursor_line_idx = buf.byte_to_line_clamped(cursor_byte);

    const SCROLL_PADDING: usize = 3;

    let max_byte_scroll = buf.len_lines().saturating_sub(1);
    buf.renderer.byte_scroll = buf.renderer.byte_scroll.min(max_byte_scroll);

    // When scroll_lines moved the viewport, clamp the cursor into the visible area
    // (with symmetric padding) rather than scrolling to follow the cursor.
    if buf.renderer.cursor_drag {
        buf.renderer.cursor_drag = false;
        buf.renderer.visual_scroll = 0;

        let scroll = buf.renderer.byte_scroll;
        let top_bound = (scroll + SCROLL_PADDING).min(max_byte_scroll);
        let bottom_bound = scroll
            .saturating_add(viewport_height)
            .saturating_sub(SCROLL_PADDING + 1)
            .min(max_byte_scroll);
        // bottom_bound can only be less than top_bound on very small viewports; clamp defensively.
        let bottom_bound = bottom_bound.max(top_bound);

        let target_line = cursor_line_idx.clamp(top_bound, bottom_bound);

        if target_line != cursor_line_idx {
            drag_cursor_to_line(&mut buf, cursor_byte, cursor_line_idx, target_line);
        }
        return;
    }

    // Normal case: scroll follows the cursor.
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
    buf.renderer.byte_scroll = buf.renderer.byte_scroll.min(max_byte_scroll);
}

/// Moves the primary cursor to `target_line`, preserving the current column.
fn drag_cursor_to_line(buf: &mut TextBuffer, cursor_byte: usize, cursor_line: usize, target_line: usize) {
    let line_start_byte = buf.line_to_byte_clamped(cursor_line);
    let current_col_char_idx =
        buf.byte_to_char_clamped(cursor_byte) - buf.byte_to_char_clamped(line_start_byte);

    let target_slice = buf.line_clamped(target_line);
    let line_len_with_ending = target_slice.len_chars();
    let endline_text = target_slice
        .chars_at(line_len_with_ending.saturating_sub(2))
        .collect::<String>();
    let line_ending_len = if endline_text.ends_with("\r\n") {
        2
    } else if endline_text.ends_with('\n') || endline_text.ends_with('\r') {
        1
    } else {
        0
    };

    let final_col = current_col_char_idx.min(line_len_with_ending.saturating_sub(line_ending_len));
    let new_caret_byte =
        buf.line_to_byte_clamped(target_line) + buf.line_clamped(target_line).char_to_byte(final_col);

    let cursor_mut = buf.primary_cursor_mut();
    cursor_mut.set_sel(new_caret_byte..=new_caret_byte);
    cursor_mut.set_at_start(false);
}
