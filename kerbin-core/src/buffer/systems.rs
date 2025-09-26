use std::collections::VecDeque;

use crate::*;
use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};

/// The default renderer for a `TextBuffer`.
///
/// This system takes a `TextBuffer` and renders its content to a `BufferChunk`,
/// handling scrolling, line numbers, basic syntax highlighting (default and selection colors),
/// and cursor display based on the current editor mode.
///
/// # Arguments
///
/// * `chunk`: `Chunk<BufferChunk>` providing mutable access to the buffer's drawing area.
/// * `theme`: `Res<Theme>` for retrieving `ContentStyle`s for text, line numbers, selections, and cursors.
/// * `modes`: `Res<ModeStack>` to determine the current editor mode for cursor styling.
/// * `bufs`: `Res<Buffers>` to access the current `TextBuffer` and its associated data.
pub async fn render_buffer_default(
    chunk: Chunk<BufferChunk>,
    theme: Res<Theme>,
    modes: Res<ModeStack>,
    bufs: Res<Buffers>,
) {
    let mut chunk = chunk.get().unwrap();
    get!(bufs, modes, theme);
    let mut loc = vec2(0, 0);

    let buf = bufs.cur_buffer();
    let buf = buf.read().unwrap();

    let mut byte_offset = buf.rope.line_to_byte_idx(buf.scroll, LineType::LF_CR);

    let cursor_byte = buf.primary_cursor().get_cursor_byte();
    let rope = &buf.rope;

    let current_row_idx = rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);
    let line_start_byte_idx = rope.line_to_byte_idx(current_row_idx, LineType::LF_CR);
    let current_col_idx = rope
        .byte_to_char_idx(cursor_byte)
        .saturating_sub(rope.byte_to_char_idx(line_start_byte_idx));

    let cursor_style_shape = match modes.get_mode() {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

    chunk.set_cursor(
        0,
        (
            current_col_idx as u16 + 6 - buf.h_scroll as u16,
            current_row_idx as u16 - buf.scroll as u16,
        )
            .into(),
        cursor_style_shape,
    );

    let default_style = theme
        .get("ui.text")
        .unwrap_or_else(|| ContentStyle::new().with(Color::Rgb { r: 0, g: 0, b: 0 }));

    let line_style = theme
        .get("ui.linenum")
        .unwrap_or(ContentStyle::new().dark_grey());

    let sel_style = theme.get("ui.selection");

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

    let primary_cursor_style = cursor_style_theme
        .or_else(|| theme.get("ui.cursor"))
        .unwrap_or_default();

    let secondary_cursor_style = theme
        .get("ui.cursor.secondary")
        .unwrap_or_else(|| primary_cursor_style.on_dark_grey());

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

        if line_idx == current_row_idx {
            num_line = format!(
                "{}{}",
                " ".repeat(4usize.saturating_sub(num_line.len())),
                num_line
            );
        } else {
            num_line = format!(
                "{}{}",
                " ".repeat(5usize.saturating_sub(num_line.len())),
                num_line
            );
        }

        render!(chunk, loc => [StyledContent::new(line_style, num_line)]);
        loc.x += gutter_width;

        let line_chars: Vec<(usize, char)> = line.char_indices().collect();

        for (char_col, (char_byte_idx, ch)) in line_chars.iter().enumerate() {
            if char_col < buf.h_scroll {
                continue;
            }

            let render_col = char_col - buf.h_scroll;
            if render_col >= visible_width as usize {
                break;
            }

            let absolute_byte_idx = byte_offset + char_byte_idx;

            let mut is_primary_cursor = false;
            let mut is_secondary_cursor = false;
            let mut in_selection = false;

            for (cursor_idx, cursor) in buf.cursors.iter().enumerate() {
                if cursor.get_cursor_byte() == absolute_byte_idx {
                    if cursor_idx == buf.primary_cursor {
                        is_primary_cursor = true;
                    } else {
                        is_secondary_cursor = true;
                    }
                }

                if cursor.sel().contains(&absolute_byte_idx)
                    && cursor.sel().start() != cursor.sel().end()
                {
                    in_selection = true;
                }
            }

            let final_style = if is_primary_cursor {
                primary_cursor_style
            } else if is_secondary_cursor {
                secondary_cursor_style
            } else if in_selection {
                sel_style
                    .map(|s| s.combined_with(&default_style))
                    .unwrap_or(default_style.on_grey())
            } else {
                default_style
            };

            render!(
                chunk,
                loc + vec2(render_col as u16, 0) =>
                [StyledContent::new(final_style, *ch)]
            );
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
///
/// # Arguments
///
/// * `chunk`: `Chunk<BufferlineChunk>` providing mutable access to the bufferline's drawing buffer.
/// * `buffers`: `Res<Buffers>` for information about open buffers and their paths.
/// * `theme`: `Res<Theme>` for styling the bufferline.
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
///
/// # Arguments
///
/// * `buffers`: `ResMut<Buffers>` for mutable access to the bufferline scroll state.
/// * `window`: `Res<WindowState>` to get the current window width.
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
///
/// # Arguments
///
/// * `window`: `Res<WindowState>` to get the current window dimensions.
/// * `buffers`: `ResMut<Buffers>` for mutable access to the active `TextBuffer`.
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
