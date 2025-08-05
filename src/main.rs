use std::{
    cell::RefCell,
    fs::File,
    rc::Rc,
    sync::{Mutex, atomic::Ordering},
    time::Duration,
};

use ascii_forge::prelude::*;

use crokey::{
    Combiner,
    crossterm::cursor::{Hide, MoveTo, SetCursorStyle, Show},
};

use crossterm::execute;

use tokio::sync::mpsc;
use tracing::Level;

use kerbin::{buffer_extensions::BufferExtension, *};

fn update_window(state: Arc<AppState>) {
    let mut window = state.window.write().unwrap();
    window.update(Duration::from_millis(10)).unwrap();
}

fn render_cursor(state: Arc<AppState>) {
    let mut window = state.window.write().unwrap();
    let buffers = state.buffers.read().unwrap();

    let mut cursor_pos = buffers.cur_buffer().borrow().cursor_pos;
    let scroll = buffers.cur_buffer().borrow().scroll;

    if scroll as u16 > cursor_pos.y {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    cursor_pos.y += 1;

    cursor_pos.y = cursor_pos
        .y
        .saturating_sub(buffers.cur_buffer().borrow().scroll as u16);

    cursor_pos.x = cursor_pos
        .x
        .saturating_sub(buffers.cur_buffer().borrow().h_scroll as u16);

    if cursor_pos.y > window.size().y {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    let cursor_style = match state.get_mode() {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

    window.buffer_mut().style_line(cursor_pos.y, |s| {
        s.on(Color::Rgb {
            r: 40,
            g: 40,
            b: 56,
        })
    });

    execute!(
        window.io(),
        MoveTo(cursor_pos.x + 6, cursor_pos.y),
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
        .open("zellix.log")
        .expect("file should be able to open");

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(Level::INFO)
        .with_writer(Mutex::new(log_file))
        .init();

    let window = Window::init().unwrap();
    handle_panics();

    let combiner = Combiner::default();

    let mut plugin_manager = ConfigManager::new().expect("Failed to create plugin manager");
    let res = plugin_manager.load_config();

    match res {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("{e}");
        }
    }

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<Box<dyn Command>>();

    let state = AppState::new(window, combiner, cmd_tx);
    state
        .buffers
        .write()
        .unwrap()
        .buffers
        .push(Rc::new(RefCell::new(TextBuffer::scratch())));

    if let Err(e) = plugin_manager.run_load_hook(state.clone()) {
        tracing::error!("Rune VM Error: {}", e);
    }

    //let mut event_stream = EventStream::new();

    while state.running.load(Ordering::Relaxed) {
        tokio::select! {
            //Some(Ok(event)) = event_stream.next() => {
                // Register the events here
            //},
            Some(cmd) = cmd_rx.recv() => {
                cmd.apply(state.clone())
            }
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                // Basic terminal render tick
                if let Err(e) = plugin_manager.run_update_hook(state.clone()) {
                    tracing::error!("Rune VM Error: {}", e);
                }

                handle_inputs(state.clone());

                update_highlights(state.clone().clone());
                render_buffers(state.clone());

                render_help_menu(state.clone());
                handle_command_palette_input(state.clone());
                render_command_palette(state.clone());
                catch_events(state.clone());

                update_buffer(state.clone());
                update_bufferline_scroll(state.clone());
                update_window(state.clone());
                render_cursor(state.clone());
            }
        }
    }

    state.shell.write().unwrap().cleanup();
    state.window.write().unwrap().restore().unwrap();
}