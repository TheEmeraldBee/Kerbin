use std::{fs::File, sync::Mutex, time::Duration, usize};

use ascii_forge::prelude::*;

use crokey::{
    Combiner,
    crossterm::{
        cursor::{MoveTo, SetCursorStyle, Show},
        *,
    },
    key,
};
use derive_more::{Deref, DerefMut};
use stategine::{prelude::*, system::into_system::IntoSystem};
use tracing::Level;

mod buffer;
use buffer::*;

mod commands;
use commands::*;

mod input;
use input::*;

mod key_check;

mod buffer_extensions;

mod plugin_manager;
use plugin_manager::*;

mod plugin_libs;

mod mode;
use mode::*;

mod command_palette;
use command_palette::*;

fn update_window(mut window: ResMut<Window>) {
    window.update(Duration::from_millis(10)).unwrap();
}

fn render_cursor(mut window: ResMut<Window>, mut buffers: ResMut<Buffers>, mode: Res<Mode>) {
    let mut cursor_pos = buffers.cur_buffer_mut().cursor_pos;
    cursor_pos.y += 1;

    let cursor_style = match mode.0 {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

    execute!(
        window.io(),
        MoveTo(cursor_pos.x, cursor_pos.y),
        cursor_style,
        Show,
    )
    .unwrap();
}

#[derive(Deref, DerefMut)]
struct Running(bool);

fn main() {
    let log_file = File::options()
        .create(true)
        .append(true)
        .write(true)
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

    let buffers = Buffers {
        selected_buffer: 0,
        buffers: vec![TextBuffer::scratch()],
    };

    let mut input_config = InputConfig::default();

    // ----------------------- //
    // Temporary Keybind Setup //
    // ----------------------- //

    // Command Palette
    input_config.register_input(
        ['n'],
        // The ':' key is Shift + Semicolon on most US layouts
        [key!(':')],
        [EditorCommand::ChangeMode('c')],
        "Command Palette",
    );

    // Movement
    input_config.register_input(
        [],
        [key!(h)],
        [EditorCommand::MoveCursor(-1, 0)],
        "Move Left",
    );
    input_config.register_input(
        [],
        [key!(j)],
        [EditorCommand::MoveCursor(0, 1)],
        "Move Down",
    );
    input_config.register_input([], [key!(k)], [EditorCommand::MoveCursor(0, -1)], "Move Up");
    input_config.register_input(
        [],
        [key!(l)],
        [EditorCommand::MoveCursor(1, 0)],
        "Move Right",
    );

    // Deletions
    input_config.register_input(
        [],
        [key!(d)],
        [
            EditorCommand::StartChangeGroup,
            EditorCommand::DeleteChars(0, 1),
            EditorCommand::CommitChangeGroup,
        ],
        "Delete",
    );
    input_config.register_input(
        [],
        [key!(shift - d)],
        [
            EditorCommand::StartChangeGroup,
            EditorCommand::DeleteChars(0, usize::MAX),
            EditorCommand::CommitChangeGroup,
        ],
        "Delete rest of line",
    );

    // Insert Mode Bindings
    input_config.register_input(
        ['i'],
        [key!(backspace)],
        [
            EditorCommand::DeleteChars(-1, 1),
            EditorCommand::MoveCursor(-1, 0),
        ],
        "Delete back",
    );
    input_config.register_input(
        ['i'],
        [key!(enter)],
        [
            EditorCommand::InsertLine(0),
            EditorCommand::MoveCursor(-i16::MAX, 1),
        ],
        "New line",
    );

    // File Commands
    input_config.register_input(
        [],
        [key!(';'), key!(w)],
        [EditorCommand::WriteFile(None)],
        "Write File",
    );

    // Mode Switching
    input_config.register_input(
        ['n'],
        [key!(i)],
        [
            EditorCommand::StartChangeGroup,
            EditorCommand::ChangeMode('i'),
        ],
        "Insert Mode",
    );
    input_config.register_input(
        [],
        [key!(esc)],
        [
            EditorCommand::CommitChangeGroup,
            EditorCommand::ChangeMode('n'),
        ],
        "Exit to normal mode",
    );

    // Buffer management
    input_config.register_input(
        ['n'],
        [key!(';'), key!(q)],
        [EditorCommand::CloseCurrentBuffer],
        "Close buffer",
    );
    input_config.register_input(
        ['n'],
        [key!(';'), key!(shift - q)],
        [EditorCommand::Quit],
        "Quit",
    );
    input_config.register_input(
        [],
        [key!(g), key!(n)],
        [EditorCommand::ChangeBuffer(1)],
        "Goto next buffer",
    );
    input_config.register_input(
        [],
        [key!(g), key!(p)],
        [EditorCommand::ChangeBuffer(-1)],
        "Goto prev buffer",
    );

    // Quick actions
    input_config.register_input(
        [],
        [key!(g), key!(h)],
        [EditorCommand::MoveCursor(-i16::MAX, 0)],
        "Goto end of line",
    );
    input_config.register_input(
        [],
        [key!(g), key!(l)],
        [EditorCommand::MoveCursor(i16::MAX, 0)],
        "Goto beginning of line",
    );

    input_config.register_input(
        [],
        [key!(shift - x)],
        [
            EditorCommand::StartChangeGroup,
            EditorCommand::DeleteLine(0),
            EditorCommand::CommitChangeGroup,
        ],
        "Delete Current Line",
    );

    // History
    input_config.register_input([], [key!(u)], [EditorCommand::Undo], "Undo");
    input_config.register_input([], [key!(shift - u)], [EditorCommand::Redo], "Redo");

    let mut plugin_manager = PluginManager::new().expect("Failed to create plugin manager");
    plugin_manager
        .load_plugins()
        .expect("Failed to load plugins");

    let mut engine = Engine::new();
    engine.states((Running(true), Mode::default()));
    engine.states((window, combiner, buffers));
    engine.states((
        input_config,
        InputState::default(),
        CommandPaletteState::new(),
        plugin_manager,
    ));

    engine.systems((
        handle_inputs,
        render_buffers,
        render_help_menu,
        handle_command_palette_input,
        render_command_palette,
        run_plugin_render_hooks,
    ));

    while engine.get_state_mut::<Running>().0 == true {
        engine.update();
        engine.oneshot_system(update_window.into_system());
        engine.oneshot_system(render_cursor.into_system());
    }

    engine.get_state_mut::<Window>().restore().unwrap();
}
