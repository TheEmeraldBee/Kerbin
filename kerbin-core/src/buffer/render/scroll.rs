use crate::*;

/// System that manages horizontal scrolling for the buffer.
///
/// `h_scroll` determines how many columns to skip when rendering each line.
/// This ensures the primary cursor remains visible with padding from left/right edges.
pub async fn update_buffer_horizontal_scroll(chunk: Chunk<BufferChunk>, buffers: ResMut<Buffers>) {
    let Some(chunk) = chunk.get().await else {
        return;
    };
    let viewport_width = chunk.size().x as usize;

    get!(mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    // Get the primary cursor's byte position
    let cursor_byte = buf.primary_cursor().get_cursor_byte();

    // Find which column the cursor is at
    let mut cursor_col = None;

    for line in &buf.renderer.lines {
        if let Some(col) = line.byte_to_col(cursor_byte) {
            cursor_col = Some(col);
            break;
        }
    }

    let Some(cursor_col) = cursor_col else { return };

    const H_SCROLL_PADDING: usize = 5;

    // Calculate where cursor appears in viewport (after h_scroll)
    let cursor_viewport_col = cursor_col.saturating_sub(buf.renderer.h_scroll);

    // If cursor is too far right, scroll right
    if cursor_viewport_col >= viewport_width.saturating_sub(H_SCROLL_PADDING) {
        let target_col = viewport_width.saturating_sub(H_SCROLL_PADDING + 1);
        let needed_scroll = cursor_viewport_col - target_col;
        buf.renderer.h_scroll += needed_scroll;
    }

    // If cursor is too far left, scroll left
    if cursor_viewport_col < H_SCROLL_PADDING {
        let needed_scroll = H_SCROLL_PADDING - cursor_viewport_col;
        if buf.renderer.h_scroll >= needed_scroll {
            buf.renderer.h_scroll -= needed_scroll;
        } else {
            buf.renderer.h_scroll = 0;
        }
    }

    // Handle cursor at absolute left edge (col 0)
    if cursor_col < buf.renderer.h_scroll {
        buf.renderer.h_scroll = cursor_col.saturating_sub(H_SCROLL_PADDING);
    }
}

/// System that manages vertical scrolling for the buffer.
///
/// `byte_scroll` determines which line `build_buffer_lines` starts rendering from.
/// `visual_scroll` is how many of those built lines we skip when displaying.
///
/// This ensures the primary cursor remains visible with a 3-line padding from edges.
pub async fn update_buffer_vertical_scroll(chunk: Chunk<BufferChunk>, buffers: ResMut<Buffers>) {
    let Some(chunk) = chunk.get().await else {
        return;
    };
    let viewport_height = chunk.size().y as usize;

    get!(mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    // Get the primary cursor's byte position
    let cursor_byte = buf.primary_cursor().get_cursor_byte();

    // Find which line (in byte terms) the cursor is on
    let cursor_line_idx = buf.rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);

    // Find which visual line (in the built lines) the cursor appears on
    let mut cursor_visual_line = None;
    let mut current_byte_line = buf.renderer.byte_scroll;

    for (idx, _line) in buf.renderer.lines.iter().enumerate() {
        if current_byte_line == cursor_line_idx {
            cursor_visual_line = Some(idx);
            break;
        }
        current_byte_line += 1;
    }

    const SCROLL_PADDING: usize = 3;

    // If cursor line isn't in our built lines at all, rebuild from a different position
    if cursor_visual_line.is_none() {
        if cursor_line_idx < buf.renderer.byte_scroll {
            // Cursor is above our built range - rebuild from earlier
            buf.renderer.byte_scroll = cursor_line_idx.saturating_sub(SCROLL_PADDING);
        } else {
            // Cursor is below our built range - rebuild to show cursor near bottom with padding
            buf.renderer.byte_scroll =
                cursor_line_idx.saturating_sub(viewport_height.saturating_sub(SCROLL_PADDING + 1));
        }
        buf.renderer.visual_scroll = 0;
        return;
    }

    let cursor_visual_line = cursor_visual_line.unwrap();

    // Calculate where the cursor appears in the viewport (after visual_scroll is applied)
    let cursor_viewport_position = cursor_visual_line.saturating_sub(buf.renderer.visual_scroll);

    // If cursor is too close to bottom of viewport, scroll down
    if cursor_viewport_position >= viewport_height.saturating_sub(SCROLL_PADDING) {
        let target_position = viewport_height.saturating_sub(SCROLL_PADDING + 1);
        let needed_scroll = cursor_viewport_position - target_position;

        // Can we scroll within our built lines?
        let max_visual_scroll = buf.renderer.lines.len().saturating_sub(viewport_height);
        let new_visual_scroll = (buf.renderer.visual_scroll + needed_scroll).min(max_visual_scroll);

        if new_visual_scroll == buf.renderer.visual_scroll && needed_scroll > 0 {
            // Hit the limit of built lines, need to rebuild from a lower position
            buf.renderer.byte_scroll += needed_scroll;
            buf.renderer.visual_scroll = 0;
        } else {
            buf.renderer.visual_scroll = new_visual_scroll;
        }
    }

    // If cursor is too close to top of viewport, scroll up
    if cursor_viewport_position < SCROLL_PADDING {
        let needed_scroll = SCROLL_PADDING - cursor_viewport_position;

        if buf.renderer.visual_scroll >= needed_scroll {
            // Can scroll up within built lines
            buf.renderer.visual_scroll -= needed_scroll;
        } else {
            // Need to rebuild from earlier lines
            let overflow = needed_scroll - buf.renderer.visual_scroll;
            buf.renderer.byte_scroll = buf.renderer.byte_scroll.saturating_sub(overflow);
            buf.renderer.visual_scroll = 0;
        }
    }

    // Clamp byte_scroll to valid range
    let max_byte_scroll = buf.rope.len_lines(LineType::LF_CR).saturating_sub(1);
    buf.renderer.byte_scroll = buf.renderer.byte_scroll.min(max_byte_scroll);
}
