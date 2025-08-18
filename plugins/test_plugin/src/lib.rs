#![allow(improper_ctypes_definitions)]

use std::sync::Arc;

use kerbin_core::*;
use kerbin_macros::*;

use ascii_forge::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuitCommand {
    Quit,
}

impl Command for QuitCommand {
    fn apply(&self, state: Arc<State>) -> bool {
        match *self {
            Self::Quit => state
                .running
                .store(false, std::sync::atomic::Ordering::Relaxed),
        }
        true
    }
}

pub fn repeat(
    commands: Vec<Box<dyn Command>>,
) -> Box<dyn Fn(Arc<State>, usize) -> bool + Send + Sync> {
    Box::new(move |state, times| {
        for _i in 0..times {
            for command in commands.iter() {
                if !command.apply(state.clone()) {
                    return false;
                }
            }
        }
        true
    })
}

#[kerbin]
pub async fn init(state: Arc<State>) {
    state
        .buffers
        .write()
        .unwrap()
        .open("kerbin/src/main.rs".to_string());

    state.register_command_deserializer::<QuitCommand>();

    state.call_command("quit");

    let mut conf = state.input_config.write().unwrap();

    conf.register_input(Input::new(
        [],
        [(KeyModifiers::NONE, KeyCode::Esc)],
        repeat(vec![Box::new(BufferCommand::InsertChar('h'))]),
        "insert h".to_string(),
    ));

    conf.register_input(Input::new(
        [],
        [(KeyModifiers::NONE, KeyCode::Char('i'))],
        Box::new(|state, _i| {
            state.set_mode('i');
            true
        }),
        "Enter Insert Mode".to_string(),
    ));

    conf.register_input(Input::new(
        [],
        [(KeyModifiers::NONE, KeyCode::Backspace)],
        Box::new(|state, _times| {
            let buffer = state.buffers.read().unwrap().cur_buffer();

            let mut cur_buffer = buffer.write().unwrap();

            let row = cur_buffer.row;
            let col = cur_buffer.col;

            if cur_buffer.col == 0 {
                cur_buffer.col = cur_buffer.cur_line().len();
                // Move the cursor up a line
                cur_buffer.move_cursor(-1, 0);

                // Join Line Here
            } else {
                if !cur_buffer.action(Delete {
                    row,
                    col: col - 1,
                    len: 1,
                }) {
                    return false;
                }

                cur_buffer.move_cursor(0, -1);
            }

            true
        }),
        "Backspace".to_string(),
    ));
}

#[kerbin]
pub async fn update(state: Arc<State>) {
    state.call_command("quit");

    render!(state.window.write().unwrap(), (0, 10) => ["Hello".red()]);
}
