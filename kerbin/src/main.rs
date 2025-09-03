use std::{fs::File, panic, sync::Mutex, time::Duration};

use ascii_forge::{
    prelude::*,
    window::crossterm::{
        cursor::{Hide, MoveTo, SetCursorStyle, Show},
        execute,
    },
};

use kerbin_config::Config;
use kerbin_core::*;

use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};
use tokio::sync::mpsc::unbounded_channel;

use tracing::Level;

#[macro_export]
macro_rules! get {
    (@inner $name:ident $(, $($t:tt)+)?) => {
        let $name = $name.get();
        get!(@inner $($($t)+)?)
    };
    (@inner mut $name:ident $(, $($t:tt)+)?) => {
        let mut $name = $name.get();
        get!(@inner $($($t)*)?)
    };
    (@inner $($t:tt)+) => {
        compile_error!("Expected comma-separated list of (mut item) or (item), but got an error while parsing. Make sure you don't have a trailing `,`");
    };
    (@inner) => {};
    ($($t:tt)*) => {
        get!(@inner $($t)*)
    };
}

pub async fn render_cursor(
    window: ResMut<WindowState>,
    buffers: Res<Buffers>,
    mode_stack: Res<ModeStack>,
) {
    get!(mut window, buffers, mode_stack);

    let current_buffer_handle = buffers.cur_buffer();
    let buffer = current_buffer_handle.read().unwrap();

    let cursor_byte = buffer.primary_cursor().get_cursor_byte();
    let rope = &buffer.rope;

    let mut current_row_idx = rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);
    let line_start_byte_idx = rope.line_to_byte_idx(current_row_idx, LineType::LF_CR);
    let mut current_col_idx = rope
        .byte_to_char_idx(cursor_byte)
        .saturating_sub(rope.byte_to_char_idx(line_start_byte_idx));

    let scroll = buffer.scroll;
    let h_scroll = buffer.h_scroll;

    if scroll > current_row_idx {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    current_row_idx = current_row_idx.saturating_sub(scroll);
    current_col_idx = current_col_idx.saturating_sub(h_scroll);

    let display_row = (current_row_idx + 1) as u16;

    if display_row > window.size().y {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    let cursor_style = match mode_stack.get_mode() {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

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

    let (command_sender, mut command_reciever) = unbounded_channel();

    let mut state = init_state(window, command_sender);

    let config = Config::load(None).unwrap();

    let plugins = config.get_plugins();

    config.apply(&mut state);

    for plugin in &plugins {
        let _: () = plugin.call_func(b"init", &mut state);
    }

    let mut commands = state.lock_state::<CommandRegistry>().unwrap();

    commands.register::<BufferCommand>();
    commands.register::<CommitCommand>();

    commands.register::<CursorCommand>();

    commands.register::<BuffersCommand>();

    commands.register::<ModeCommand>();
    commands.register::<StateCommand>();

    commands.register::<MotionCommand>();

    commands.register::<ShellCommand>();

    drop(commands);

    state
        .on_hook(Update)
        .system(handle_inputs)
        .system(handle_command_palette_input)
        .system(update_palette_suggestions)
        .system(update_buffer);

    state
        .on_hook(Render)
        .system(render_statusline)
        .system(render_command_palette)
        .system(render_help_menu)
        .system(render_cursor);

    state
        .on_hook(RenderFiletype::new("rs | toml"))
        .system(render_buffers);

    state.hook(PostInit).call().await;

    loop {
        tokio::select! {
            Some(cmd) = command_reciever.recv() => {
                cmd.apply(&mut state);
            }
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                // Update all states
                state.hook(Update).call().await;

                // Call the file renderer
                state.hook(RenderFiletype::new("py")).call().await;

                // Render the rest of the state
                state
                    .hook(Render)
                    .call()
                    .await;

                state
                    .lock_state::<WindowState>()
                    .unwrap()
                    .update(Duration::from_millis(0))
                    .unwrap();

                if !state.lock_state::<Running>().unwrap().0 {
                    break
                }
            }
        };
    }

    // Plugins must survive until program exit
    for plugin in plugins.into_iter() {
        std::mem::forget(plugin);
    }

    state
        .lock_state::<WindowState>()
        .unwrap()
        .restore()
        .expect("Window should restore fine");
}
