use std::collections::VecDeque;

use crate::*;
use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};

pub async fn render_cursors_and_selections(
    bufs: ResMut<Buffers>,
    modes: Res<ModeStack>,
    theme: Res<Theme>,
) {
    get!(mut bufs, modes, theme);

    let mut buf = bufs.cur_buffer_mut().await;

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
                'i' => SetCursorStyle::BlinkingBar,
                _ => SetCursorStyle::SteadyBlock,
            };

            buf.add_extmark(
                ExtmarkBuilder::new("inner::cursor", caret_byte).with_decoration(
                    ExtmarkDecoration::Cursor {
                        style: cursor_style,
                    },
                ),
            );
        } else {
            buf.add_extmark(
                ExtmarkBuilder::new("inner::cursor", caret_byte)
                    .with_decoration(ExtmarkDecoration::Highlight { hl: cursor_style }),
            );
        }

        if cursor.sel().start() != cursor.sel().end() {
            buf.add_extmark(
                ExtmarkBuilder::new_range(
                    "inner::selection",
                    *cursor.sel().start()..*cursor.sel().end(),
                )
                .with_decoration(ExtmarkDecoration::Highlight { hl: sel_style }),
            );
        }
    }
}

pub async fn render_bufferline(
    chunk: Chunk<BufferlineChunk>,
    buffers: Res<Buffers>,
    theme: Res<Theme>,
) {
    let chunk = &mut chunk.get().await.unwrap();
    get!(buffers, theme);

    buffers.render_bufferline(chunk, &theme).await;
}

pub async fn update_bufferline_scroll(buffers: ResMut<Buffers>, window: Res<WindowState>) {
    get!(mut buffers, window);

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

pub async fn post_update_buffer(buffers: ResMut<Buffers>) {
    get!(mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    buf.post_update();
}

pub async fn cleanup_buffers(buffers: ResMut<Buffers>) {
    get!(mut buffers);

    buffers.update_paths().await;

    let mut buffer = buffers.cur_buffer_mut().await;

    buffer.update_cleanup();
}
