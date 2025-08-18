use std::{
    sync::{Arc, atomic::Ordering},
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

fn render_cursor(state: Arc<State>) {
    let mut window = state.window.write().unwrap();
    let buffers = state.buffers.read().unwrap();

    let mut row = buffers.cur_buffer().read().unwrap().row;
    let mut col = buffers.cur_buffer().read().unwrap().col;
    let scroll = buffers.cur_buffer().read().unwrap().scroll;

    if scroll > row {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    row += 1;

    row = row.saturating_sub(buffers.cur_buffer().read().unwrap().scroll);

    col = col.saturating_sub(buffers.cur_buffer().read().unwrap().h_scroll);

    if row > window.size().y as usize {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    let cursor_style = match state.get_mode() {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

    execute!(
        window.io(),
        MoveTo(col as u16 + 6, row as u16),
        cursor_style,
        Show,
    )
    .unwrap();
}

#[tokio::main]
async fn main() {
    handle_panics();
    let window = Window::init().unwrap();

    let my_plugin = Plugin::load("./target/release/libtest_plugin.so");

    let (command_sender, mut command_reciever) = unbounded_channel();

    let state = Arc::new(State::new(window, command_sender));
    my_plugin
        .call_async_func::<_, ()>(b"init\0", state.clone())
        .await;

    // Register Command States
    state.register_command_deserializer::<BufferCommand>();
    state.register_command_deserializer::<ModeCommand>();
    state.register_command_deserializer::<StateCommand>();

    let config = Config::load(None).unwrap();
    config.apply(state.clone());

    loop {
        tokio::select! {
            Some(cmd) = command_reciever.recv() => {
                cmd.apply(state.clone());
            }
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                // Basic Frame update

                state.buffers.write().unwrap().render(vec2(0, 0), state.window.write().unwrap().buffer_mut(), &state.theme.read().unwrap());

                my_plugin.call_async_func::<_, ()>(b"update\0", state.clone()).await;

                handle_inputs(state.clone());
                render_help_menu(state.clone());

                update_buffer(state.clone());

                state.window.write().unwrap().update(Duration::ZERO).unwrap();
                render_cursor(state.clone());

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
