use std::collections::VecDeque;

use crate::*;
use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Projects `Cursor`s and their selections into temporary extmarks for rendering.
///
/// This system clears ephemeral extmarks each frame and recreates them from
/// the current cursor state, allowing `render_buffer_default` to render them
/// as part of the extmark pipeline.
pub async fn render_cursors_and_selections(
    bufs: Res<Buffers>,
    modes: Res<ModeStack>,
    theme: Res<Theme>,
) {
    get!(bufs, modes, theme);

    let buf_arc = bufs.cur_buffer();
    let mut buf = buf_arc.write().unwrap();

    buf.clear_extmark_ns("inner::cursor");
    buf.clear_extmark_ns("inner::selection");

    let mut cursor_parts = modes
        .0
        .iter()
        .map(|x| x.to_string())
        .collect::<VecDeque<_>>();

    let mut cursor_style_theme = None;

    while !cursor_parts.is_empty() {
        if let Some(s) = theme.get(&format!(
            "ui.cursor.{}",
            cursor_parts
                .iter()
                .cloned()
                .reduce(|l, r| format!("{l}.{r}"))
                .unwrap()
        )) {
            cursor_style_theme = Some(s);
            break;
        }
        cursor_parts.pop_front();
    }

    let cursor_style = cursor_style_theme
        .or_else(|| theme.get("ui.cursor"))
        .unwrap_or_default();

    let sel_style = theme
        .get("ui.selection")
        .unwrap_or(ContentStyle::new().on_grey());

    let primary_cursor = buf.primary_cursor;
    for (i, cursor) in buf.cursors.clone().into_iter().enumerate() {
        let caret_byte = cursor.get_cursor_byte();

        if primary_cursor == i {
            buf.add_extmark(
                "inner::cursor",
                caret_byte,
                0,
                vec![ExtmarkDecoration::Cursor {
                    style: SetCursorStyle::SteadyBlock,
                }],
            );
        } else {
            buf.add_extmark(
                "inner::cursor",
                caret_byte,
                0,
                vec![ExtmarkDecoration::Highlight { hl: cursor_style }],
            );
        }

        if cursor.sel().start() != cursor.sel().end() {
            buf.add_extmark_range(
                "inner::selection",
                *cursor.sel().start()..*cursor.sel().end(),
                1,
                vec![ExtmarkDecoration::Highlight { hl: sel_style }],
            );
        }
    }
}

/// Default renderer for a `TextBuffer`.
///
/// This function renders:
/// - The rope text itself
/// - Any extmarks in the visible viewport (highlights, ghost text, virtual lines, widgets, cursors)
///
/// Cursor positions and selections are *not* hardcoded here — they are provided
/// each frame as [`Extmark`]s by the [`render_cursors_and_selections`] system.
pub async fn render_buffer_default(
    chunk: Chunk<BufferChunk>,
    theme: Res<Theme>,
    bufs: Res<Buffers>,
) {
    let mut chunk = chunk.get().unwrap();
    get!(bufs, theme);
    let mut loc = vec2(0, 0);

    let buf = bufs.cur_buffer();
    let buf = buf.read().unwrap();

    let mut byte_offset = buf.rope.line_to_byte_idx(buf.scroll, LineType::LF_CR);

    // Style lookups
    let default_style = theme
        .get("ui.text")
        .unwrap_or_else(|| ContentStyle::new().with(Color::Rgb { r: 0, g: 0, b: 0 }));
    let line_style = theme
        .get("ui.linenum")
        .unwrap_or(ContentStyle::new().dark_grey());

    let gutter_width = 6;
    let start_x = loc.x;
    let visible_width = chunk.size().x.saturating_sub(gutter_width);

    let mut line_idx = buf.scroll;

    for line in buf
        .rope
        .lines_at(buf.scroll, LineType::LF_CR)
        .take(chunk.size().y as usize)
    {
        loc.x = start_x;

        let mut num_line = (line_idx + 1).to_string();
        if num_line.len() > 5 {
            num_line = num_line[0..5].to_string();
        }
        num_line = format!(
            "{}{}",
            " ".repeat(5usize.saturating_sub(num_line.len())),
            num_line
        );

        render!(chunk, loc => [StyledContent::new(line_style, num_line)]);
        loc.x += gutter_width;

        let mut line_chars: Vec<(usize, char)> = line.char_indices().collect();
        let line_start_byte = buf.rope.line_to_byte_idx(line_idx, LineType::LF_CR);
        let line_end_byte = buf.rope.line_to_byte_idx(line_idx + 1, LineType::LF_CR);

        if let Some((_, ch)) = line_chars.last() {
            if *ch == '\n' || *ch == '\r' {
                line_chars.pop();
            }
        }

        if line_chars.is_empty() {
            line_chars.push((0, ' '));
        } else {
            line_chars.push((line.len().saturating_sub(1).max(1), ' '));
        }

        let exts = buf.query_extmarks(line_start_byte..line_end_byte);

        let mut col_count = 0;

        for (char_byte_idx, ch) in line_chars.iter() {
            let absolute_byte_idx = byte_offset + char_byte_idx;
            let mut style = default_style;

            for ext in &exts {
                for deco in &ext.decorations {
                    match deco {
                        ExtmarkDecoration::Cursor { style } => {
                            if ext.byte_range.start == absolute_byte_idx {
                                chunk.set_cursor(
                                    0,
                                    (loc.x + col_count as u16, loc.y).into(),
                                    *style,
                                );
                            }
                        }
                        ExtmarkDecoration::Highlight { hl } => {
                            if ext.byte_range.contains(&absolute_byte_idx) {
                                style = style.combined_with(hl);
                            }
                        }
                        ExtmarkDecoration::VirtText { text, hl } => {
                            if ext.byte_range.start == absolute_byte_idx {
                                if col_count >= buf.h_scroll {
                                    let render_col = col_count - buf.h_scroll;
                                    if render_col < visible_width as usize {
                                        let style = hl.unwrap_or(ContentStyle::new().dark_grey());
                                        render!(
                                            chunk,
                                            loc + vec2(render_col as u16, 0) =>
                                            [StyledContent::new(style, text.clone())]
                                        );
                                    }
                                }
                                col_count += text.width();
                            }
                        }
                    }
                }
            }

            if col_count >= buf.h_scroll {
                let render_col = col_count - buf.h_scroll;
                if render_col < visible_width as usize {
                    render!(
                        chunk,
                        loc + vec2(render_col as u16, 0) =>
                        [StyledContent::new(style, ch)]
                    );
                }
            }

            col_count += ch.width().unwrap_or(1);
        }

        loc.y += 1;
        byte_offset += line.len();
        line_idx += 1;
    }
}

/// System used to render the bufferline (tab bar) to the `BufferlineChunk`.
///
/// This system retrieves the `Buffers` and `Theme` resources and delegates
/// the actual rendering to the `Buffers::render_bufferline` method.
pub async fn render_bufferline(
    chunk: Chunk<BufferlineChunk>,
    buffers: Res<Buffers>,
    theme: Res<Theme>,
) {
    let chunk = &mut chunk.get().unwrap();
    let buffers = buffers.get();
    let theme = theme.get();

    buffers.render_bufferline(chunk, &theme);
}

/// System that updates the horizontal scroll position of the bufferline.
///
/// This system ensures that the currently selected buffer's tab is always
/// visible within the bufferline display area, adjusting `tab_scroll` as needed.
pub async fn update_bufferline_scroll(buffers: ResMut<Buffers>, window: Res<WindowState>) {
    let mut buffers = buffers.get();
    let window = window.get();

    if buffers.buffers.is_empty() {
        buffers.tab_scroll = 0;
        return;
    }

    // Calculate width of each tab (path + padding)
    let tab_widths: Vec<usize> = buffers.buffer_paths.iter().map(|p| p.len() + 6).collect();

    // Calculate starting character offset for each tab
    let tab_starts: Vec<usize> = tab_widths
        .iter()
        .scan(0, |acc, &w| {
            let start = *acc;
            *acc += w;
            Some(start)
        })
        .collect();

    let selected_idx = buffers.selected_buffer;
    let selected_tab_start = tab_starts[selected_idx];
    let selected_tab_end = selected_tab_start + tab_widths[selected_idx];

    let view_width = window.size().x as usize;
    let view_start = buffers.tab_scroll;
    let view_end = view_start + view_width;

    // Adjust scroll if the selected tab extends beyond the right edge
    if selected_tab_end > view_end {
        buffers.tab_scroll = selected_tab_end.saturating_sub(view_width);
    }

    // Adjust scroll if the selected tab starts before the left edge
    if selected_tab_start < view_start {
        buffers.tab_scroll = selected_tab_start;
    }

    // Ensure tab_scroll doesn't allow scrolling past the total content width
    let total_width: usize = tab_widths.iter().sum();
    if total_width < view_width {
        // If all tabs fit, reset scroll to 0
        buffers.tab_scroll = 0;
    } else {
        // Otherwise, clamp scroll to prevent empty space on the right
        buffers.tab_scroll = buffers
            .tab_scroll
            .min(total_width.saturating_sub(view_width));
    }
}

/// System that updates the active buffer's state, including its content,
/// and handles horizontal and vertical scrolling to keep the primary cursor in view.
///
/// This system is crucial for ensuring the displayed buffer content is up-to-date
/// and the user's cursor remains visible as they navigate and edit.
pub async fn update_buffer(window: Res<WindowState>, buffers: ResMut<Buffers>) {
    let window = window.get();
    let mut buffers = buffers.get();

    // Re-calculate unique paths for all buffers
    buffers.update_paths();

    // Determine the visible viewport dimensions for the text buffer (excluding UI elements)
    let viewport_height = window.size().y.saturating_sub(3); // Example: 1 for bufferline, 1 for cmdline, 1 for statusline
    let viewport_width = window.size().x.saturating_sub(7); // Example: some padding

    let buffer = buffers.cur_buffer();
    let mut buffer = buffer.write().unwrap(); // Acquire write lock for the current buffer

    // Update the buffer's internal state (e.g., syntax highlighting edits)
    buffer.update();

    // Get primary cursor's byte index
    let primary_cursor_byte = buffer.primary_cursor().get_cursor_byte();

    // Calculate current row and column based on the cursor byte index
    let current_row = buffer
        .rope
        .byte_to_line_idx(primary_cursor_byte, LineType::LF_CR);
    let line_start_byte_idx = buffer.rope.line_to_byte_idx(current_row, LineType::LF_CR);
    let current_col = buffer
        .rope
        .byte_to_char_idx(primary_cursor_byte)
        .saturating_sub(buffer.rope.byte_to_char_idx(line_start_byte_idx));

    // Vertical scrolling: Adjust `buffer.scroll` to keep `current_row` visible
    if current_row < buffer.scroll {
        buffer.scroll = current_row;
    }
    if current_row >= buffer.scroll + viewport_height as usize {
        buffer.scroll = current_row.saturating_sub(viewport_height as usize) + 1;
    }

    // Horizontal scrolling: Adjust `buffer.h_scroll` to keep `current_col` visible
    if current_col < buffer.h_scroll {
        buffer.h_scroll = current_col;
    }
    if current_col >= buffer.h_scroll + viewport_width as usize {
        buffer.h_scroll = current_col.saturating_sub(viewport_width as usize) + 1;
    }
}
