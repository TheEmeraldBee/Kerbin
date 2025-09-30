use std::collections::VecDeque;

use crate::*;
use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};

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

    buf.renderer.clear_extmark_ns("inner::cursor");
    buf.renderer.clear_extmark_ns("inner::selection");

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
            let cursor_style = match modes.get_mode() {
                'i' => SetCursorStyle::SteadyBar,
                'v' => SetCursorStyle::SteadyUnderScore,
                _ => SetCursorStyle::SteadyBlock,
            };

            buf.renderer.add_extmark(
                "inner::cursor",
                caret_byte,
                0,
                vec![ExtmarkDecoration::Cursor {
                    style: cursor_style,
                }],
            );
        } else {
            buf.renderer.add_extmark(
                "inner::cursor",
                caret_byte,
                0,
                vec![ExtmarkDecoration::Highlight { hl: cursor_style }],
            );
        }

        if cursor.sel().start() != cursor.sel().end() {
            buf.renderer.add_extmark_range(
                "inner::selection",
                *cursor.sel().start()..*cursor.sel().end(),
                1,
                vec![ExtmarkDecoration::Highlight { hl: sel_style }],
            );
        }
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
pub async fn update_buffer(buffers: ResMut<Buffers>) {
    get!(mut buffers);

    // Re-calculate unique paths for all buffers
    buffers.update_paths();

    let buffer = buffers.cur_buffer();
    let mut buffer = buffer.write().unwrap(); // Acquire write lock for the current buffer

    // Update the buffer's internal state (e.g., syntax highlighting edits)
    buffer.update();
}
