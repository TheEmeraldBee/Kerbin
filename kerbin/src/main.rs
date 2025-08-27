use std::{
    fs::File,
    sync::{Arc, Mutex, RwLock, atomic::Ordering},
    time::Duration,
};

use ascii_forge::{
    prelude::*,
    window::crossterm::{
        cursor::{Hide, MoveTo, SetCursorStyle, Show},
        execute,
    },
};

use kerbin_config::Config;
use kerbin_core::*;
use kerbin_plugin::Plugin;
use tokio::sync::mpsc::unbounded_channel;
use tracing::Level;

pub fn render_cursor(state: Arc<State>) {
    let mut window = state.window.write().unwrap();
    let buffers_guard = state.buffers.read().unwrap();
    let current_buffer_handle = buffers_guard.cur_buffer();
    let buffer = current_buffer_handle.read().unwrap();

    // Calculate current row and col based on the cursor byte index
    let cursor_byte = buffer.primary_cursor().get_cursor_byte();
    let rope = &buffer.rope;

    let mut current_row_idx = rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);
    let line_start_byte_idx = rope.line_to_byte_idx(current_row_idx, LineType::LF_CR);
    let mut current_col_idx = rope
        .byte_to_char_idx(cursor_byte)
        .saturating_sub(rope.byte_to_char_idx(line_start_byte_idx));

    let scroll = buffer.scroll;
    let h_scroll = buffer.h_scroll;

    // Apply vertical scroll
    if scroll > current_row_idx {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    current_row_idx = current_row_idx.saturating_sub(scroll);
    current_col_idx = current_col_idx.saturating_sub(h_scroll);

    // Adjust row for 1-based display and header/footer offset if any
    // Assuming the editor UI starts rendering content at y=1 (after header)
    // and line numbers might take up 1 column.
    // The previous code had `row += 1;`
    let display_row = (current_row_idx + 1) as u16; // Add 1 for 1-based indexing of screen rows

    // Check if cursor is off-screen vertically
    if display_row > window.size().y {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    // Determine cursor style based on mode
    let cursor_style = match state.get_mode() {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock, // Default to block for normal mode
    };

    // `col as u16 + 6` assumes a gutter of 6 characters for line numbers.
    // Ensure `display_row` and `current_col_idx` are cast to `u16`.
    execute!(
        window.io(),
        MoveTo(current_col_idx as u16 + 6, display_row),
        cursor_style,
        Show,
    )
    .unwrap();
}

#[tokio::main]
async fn main() {
    let log_file = File::options()
        .create(true)
        .append(true)
        .open("kerbin.log")
        .expect("file should be able to open");

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(Level::INFO)
        .with_writer(Mutex::new(log_file))
        .init();

    handle_panics();
    let window = Window::init().unwrap();

    let my_plugin = Plugin::load("./target/release/libtest_plugin.so");

    let (command_sender, mut command_reciever) = unbounded_channel();

    let state = Arc::new(State::new(window, command_sender));
    state
        .buffers
        .write()
        .unwrap()
        .buffers
        .push(Arc::new(RwLock::new(TextBuffer::scratch())));

    let config = Config::load(None).unwrap();
    config.apply(state.clone());

    // Register Command States
    state.register_command_deserializer::<BufferCommand>();
    state.register_command_deserializer::<CommitCommand>();

    state.register_command_deserializer::<CursorCommand>();

    state.register_command_deserializer::<BuffersCommand>();

    state.register_command_deserializer::<ModeCommand>();
    state.register_command_deserializer::<StateCommand>();

    state.register_command_deserializer::<MotionCommand>();

    state.register_command_deserializer::<ShellCommand>();

    my_plugin
        .call_async_func::<_, ()>(b"init\0", state.clone())
        .await;

    loop {
        tokio::select! {
            Some(cmd) = command_reciever.recv() => {
                cmd.apply(state.clone());
            }
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                // Basic Frame update
                my_plugin.call_async_func::<_, ()>(b"update\0", state.clone()).await;

                handle_inputs(state.clone());

                update_palette_suggestions(state.clone());

                update_buffer(state.clone());

                state.buffers.write().unwrap().render(vec2(0, 0), state.window.write().unwrap().buffer_mut(), &state.theme.read().unwrap());

                render_command_palette(state.clone());
                render_help_menu(state.clone());

                render_cursor(state.clone());
                state.window.write().unwrap().update(Duration::ZERO).unwrap();

                if !state.running.load(Ordering::Relaxed) {
                    break
                }
            }
        };
    }

    // Clean up the state
    state
        .window
        .write()
        .unwrap()
        .restore()
        .expect("Window should restore fine");
}
