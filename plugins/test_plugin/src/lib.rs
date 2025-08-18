#![allow(improper_ctypes_definitions)]

use std::sync::Arc;

use kerbin_core::*;
use kerbin_macros::*;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomCommand {
    Backspace,
}

impl Command for CustomCommand {
    fn apply(&self, state: Arc<State>) -> bool {
        match *self {
            Self::Backspace => {
                let cur_buf = state.buffers.read().unwrap().cur_buffer();
                let mut cur_buf = cur_buf.write().unwrap();

                let row = cur_buf.row;

                if cur_buf.col == 0 {
                    cur_buf.move_cursor(0, -isize::MAX);
                    let line_len = cur_buf.lines[row.saturating_sub(1)].len();

                    // Join Line When Implemented Here
                    cur_buf.action(JoinLine {
                        row: row.saturating_sub(1),
                        undo_indent: None,
                    });
                    cur_buf.move_cursor(-1, line_len as isize);

                    true
                } else {
                    let col = cur_buf.col - 1;
                    let res = cur_buf.action(Delete { row, col, len: 1 });

                    cur_buf.move_cursor(0, -1);
                    res
                }
            }
        }
    }
}

pub fn repeat_commands(
    commands: impl IntoIterator<Item = String>,
) -> Box<dyn Fn(Arc<State>, usize) -> bool + Send + Sync> {
    let commands: Vec<_> = commands.into_iter().collect();
    Box::new(move |state, times| {
        for _i in 0..times {
            for command in &commands {
                if !state.call_command(command) {
                    return false;
                }
            }
        }
        true
    })
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
    state.register_command_deserializer::<CustomCommand>();

    // Register Grammars
    // state
    //     .grammar
    //     .write()
    //     .unwrap()
    //     .register_extension("rs", "rust");

    // state.buffers.write().unwrap().open(
    //     "kerbin/src/main.rs".to_string(),
    //     &mut state.grammar.write().unwrap(),
    //     &state.theme.read().unwrap(),
    // );
}

#[kerbin]
pub async fn update(_state: Arc<State>) {}
