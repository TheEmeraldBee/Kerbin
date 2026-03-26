use std::collections::VecDeque;

use crate::*;
use ratatui::style::{Color, Style};
use unicode_segmentation::UnicodeSegmentation;

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
        .unwrap_or(Style::default().bg(Color::Gray));

    let primary_cursor = buf.primary_cursor;
    for (i, cursor) in buf.cursors.clone().into_iter().enumerate() {
        let caret_byte = cursor.get_cursor_byte();

        if primary_cursor == i {
            let shape = match modes.get_mode() {
                'i' => CursorShape::BlinkingBar,
                _ => CursorShape::Block,
            };

            buf.add_extmark(ExtmarkBuilder::new("inner::cursor", caret_byte).with_kind(
                ExtmarkKind::Cursor {
                    style: cursor_style,
                    shape,
                },
            ));
        } else {
            buf.add_extmark(ExtmarkBuilder::new("inner::cursor", caret_byte).with_kind(
                ExtmarkKind::Highlight {
                    style: cursor_style,
                },
            ));
        }

        if cursor.sel().start() != cursor.sel().end() {
            buf.add_extmark(
                ExtmarkBuilder::new_range(
                    "inner::selection",
                    *cursor.sel().start()..*cursor.sel().end(),
                )
                .with_kind(ExtmarkKind::Highlight { style: sel_style }),
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

    let tab_widths: Vec<usize> = buffers.buffer_paths.iter().map(|p| p.width() + 6).collect();

    // Cumulative start offset for each tab
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

    let view_width = window.size().width as usize;
    let view_start = buffers.tab_scroll;
    let view_end = view_start + view_width;

    if selected_tab_end > view_end {
        buffers.tab_scroll = selected_tab_end.saturating_sub(view_width);
    }

    if selected_tab_start < view_start {
        buffers.tab_scroll = selected_tab_start;
    }

    let total_width: usize = tab_widths.iter().sum();
    if total_width < view_width {
        buffers.tab_scroll = 0;
    } else {
        // Clamp to prevent empty space on the right
        buffers.tab_scroll = buffers
            .tab_scroll
            .min(total_width.saturating_sub(view_width));
    }
}

pub async fn post_update_buffer(buffers: ResMut<Buffers>) {
    get!(mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    resolver_engine_mut()
        .await
        .set_template("cur_buf", buf.path.clone());

    buf.post_update();
}

pub async fn update_tab_width_template(buffers: Res<Buffers>) {
    get!(buffers);
    if buffers.buffers.is_empty() {
        return;
    }

    let tab_unit = match &buffers.cur_buffer().await.indent_style {
        IndentStyle::Tabs => "\t".to_string(),
        IndentStyle::Spaces(n) => " ".repeat(*n),
    };
    resolver_engine_mut()
        .await
        .set_template("tab_unit", tab_unit);
}

pub async fn cleanup_buffers(buffers: ResMut<Buffers>) {
    get!(mut buffers);

    buffers.update_paths().await;

    let mut buffer = buffers.cur_buffer_mut().await;

    buffer.update_cleanup();
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_mouse_events(
    events: Res<CrosstermEvents>,
    chunks: Res<Chunks>,
    buffers: Res<Buffers>,
    mouse_bindings: Res<MouseBindings>,
    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,
    command_sender: ResMut<CommandSender>,
    modes: Res<ModeStack>,
    core_config: Res<CoreConfig>,
) {
    use crossterm::event::{MouseButton, MouseEventKind};

    get!(events, chunks, buffers, mouse_bindings, modes, core_config);

    if events.0.is_empty() {
        return;
    }

    for event in &events.0 {
        let crossterm::event::Event::Mouse(mouse_ev) = event else {
            continue;
        };

        let trigger = match mouse_ev.kind {
            MouseEventKind::Down(MouseButton::Left) => MouseTrigger::LeftDown,
            MouseEventKind::Up(MouseButton::Left) => MouseTrigger::LeftUp,
            MouseEventKind::Down(MouseButton::Right) => MouseTrigger::RightDown,
            MouseEventKind::Up(MouseButton::Right) => MouseTrigger::RightUp,
            MouseEventKind::Down(MouseButton::Middle) => MouseTrigger::MiddleDown,
            MouseEventKind::ScrollUp => MouseTrigger::ScrollUp,
            MouseEventKind::ScrollDown => MouseTrigger::ScrollDown,
            _ => continue,
        };

        let col = mouse_ev.column;
        let row = mouse_ev.row;

        let area_opt = chunks.rect_for_chunk(&BufferChunk::static_name());

        if let Some(area) = area_opt {
            let buf = buffers.cur_buffer().await;

            let line_idx = (row.saturating_sub(area.y) as usize)
                .saturating_add(buf.renderer.byte_scroll)
                .min(buf.len_lines().saturating_sub(1));

            let line_start_byte = buf.line_to_byte_clamped(line_idx);
            let line_end_byte = buf
                .line_to_byte(line_idx + 1)
                .unwrap_or(buf.rope.len_bytes());
            let target_display_col =
                (col.saturating_sub(area.x) as usize).saturating_add(buf.renderer.h_scroll);

            let line_text = buf
                .slice_to_string(line_start_byte, line_end_byte)
                .unwrap_or_default();

            let tab_w = core_config.tab_display_unit.chars().count();
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
                    tab_w
                } else if g.contains('\u{FE0F}') {
                    2
                } else {
                    UnicodeWidthStr::width(g)
                };
                current_width += w;
                byte_offset += g.len();
            }

            drop(buf);

            let mut engine = resolver_engine_mut().await;
            engine.set_template("mouse_line", line_idx.to_string());
            engine.set_template("mouse_col", byte_offset.to_string());
            drop(engine);
        }

        let Some(commands) = mouse_bindings.bindings.get(&trigger) else {
            continue;
        };
        let commands = commands.clone();

        let resolver = resolver_engine().await;
        let resolver = resolver.as_resolver();

        let registry = prefix_registry.get().await;
        for cmd_str in &commands {
            let command = command_registry.get().await.parse_command(
                tokenize(cmd_str).unwrap_or_default(),
                true,
                false,
                Some(&resolver),
                true,
                &registry,
                &modes,
            );
            if let Some(command) = command {
                command_sender.get().await.send(command).unwrap();
            }
        }
    }
}
