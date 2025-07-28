use std::{fs::File, sync::Mutex, time::Duration, usize};

use ascii_forge::prelude::*;
use crokey::{Combiner, key};
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
use key_check::KeyCheckExt;

mod mode;
use mode::*;

fn check_quit(window: Res<Window>, mut combiner: ResMut<Combiner>, mut running: ResMut<Running>) {
    if window.combination(&mut combiner, crokey::key!(ctrl - c)) {
        running.0 = false;
    }
}

fn update_window(mut window: ResMut<Window>) {
    window.update(Duration::from_millis(10)).unwrap();
}

#[derive(Deref, DerefMut)]
struct Running(bool);

fn main() {
    let log_file = File::create("info.log").expect("Failed to create log file");

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_writer(Mutex::new(log_file))
        .init();

    let window = Window::init().unwrap();
    handle_panics();

    let combiner = Combiner::default();

    let buffers = Buffers {
        selected_buffer: 0,
        buffers: vec![TextBuffer::open("hello.txt"), TextBuffer::scratch()],
    };

    let mut input_config = InputConfig::default();

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
    input_config.register_input([], [key!(ctrl - r)], [EditorCommand::Redo], "Redo");

    let mut engine = Engine::new();
    engine.states((Running(true), Mode::default()));
    engine.states((window, combiner, buffers));
    engine.states((input_config, InputState::default()));

    engine.systems((handle_inputs, render_buffers, render_help_menu));

    engine.systems((check_quit,));

    while engine.get_state_mut::<Running>().0 == true {
        engine.update();
        engine.oneshot_system(update_window.into_system());
    }

    engine.get_state_mut::<Window>().restore().unwrap();
}
