use serde::Deserialize;

use crate::*;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferCommand {
    MoveCursor { cols: isize, rows: isize },
    ChangeMode(char),
    InsertChar(char),
    DeleteChars { offset: isize, count: usize },
}

impl Command for BufferCommand {
    fn apply(&self, state: std::sync::Arc<crate::State>) -> bool {
        let buffers = state.buffers.read().unwrap();

        let cur_buffer = buffers.cur_buffer();
        let mut cur_buffer = cur_buffer.write().unwrap();

        match *self {
            BufferCommand::MoveCursor { rows, cols } => cur_buffer.move_cursor(rows, cols),
            BufferCommand::ChangeMode(new_mode) => {
                state.set_mode(new_mode);
                true
            }
            BufferCommand::InsertChar(chr) => {
                let row = cur_buffer.row;
                let col = cur_buffer.col;

                cur_buffer.action(Insert {
                    row,
                    col,
                    content: chr.to_string(),
                })
            }
            BufferCommand::DeleteChars { offset, count } => {
                let row = cur_buffer.row;
                let col = cur_buffer.col;

                cur_buffer.action(Delete {
                    row,
                    col: col.saturating_add_signed(offset),
                    len: count,
                })
            }
        }
    }
}
