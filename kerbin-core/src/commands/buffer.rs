use std::sync::Arc;

use serde::Deserialize;

use crate::*;

#[derive(Clone, Debug)]
pub enum CommitCommand {
    Commit(Option<Vec<String>>),
}

impl Command for CommitCommand {
    fn apply(&self, state: std::sync::Arc<State>) -> bool {
        match self {
            CommitCommand::Commit(after) => {
                let mut res = true;
                // Begin the change
                state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .write()
                    .unwrap()
                    .start_change_group();

                if let Some(after) = after {
                    res = state.call_command(&after.join(" "));
                }

                // End the change
                state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .write()
                    .unwrap()
                    .commit_change_group();

                res
            }
        }
    }
}

impl CommandFromStr for CommitCommand {
    fn from_str(val: &[String]) -> Option<Box<dyn Command>> {
        match val[0].as_str() {
            "commit" => {
                if val.len() > 1 {
                    Some(Box::new(CommitCommand::Commit(Some(
                        val[1..].iter().map(|x| x.clone()).collect(),
                    ))))
                } else {
                    Some(Box::new(CommitCommand::Commit(None)))
                }
            }
            _ => return None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferCommand {
    MoveCursor { cols: isize, rows: isize },

    StartChange,
    CommitChange,

    InsertChar(char),
    DeleteChars { offset: isize, count: usize },

    JoinLine(isize),
    InsertNewline(isize),

    Undo,
    Redo,
}

impl Command for BufferCommand {
    fn apply(&self, state: std::sync::Arc<crate::State>) -> bool {
        let buffers = state.buffers.read().unwrap();

        let cur_buffer = buffers.cur_buffer();
        let mut cur_buffer = cur_buffer.write().unwrap();

        match *self {
            BufferCommand::MoveCursor { rows, cols } => cur_buffer.move_cursor(rows, cols),

            BufferCommand::StartChange => {
                cur_buffer.start_change_group();
                true
            }

            BufferCommand::CommitChange => {
                cur_buffer.commit_change_group();
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

            BufferCommand::JoinLine(offset) => {
                let row = cur_buffer.row.saturating_add_signed(offset);

                cur_buffer.action(JoinLine {
                    row,
                    undo_indent: None,
                })
            }
            BufferCommand::InsertNewline(offset) => {
                let row = cur_buffer.row;
                let col = cur_buffer.col.saturating_add_signed(offset);

                cur_buffer.action(InsertNewline { row, col })
            }

            BufferCommand::Undo => {
                cur_buffer.undo();
                true
            }
            BufferCommand::Redo => {
                cur_buffer.redo();
                true
            }
        }
    }
}
