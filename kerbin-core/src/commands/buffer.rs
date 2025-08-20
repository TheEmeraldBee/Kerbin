use kerbin_macros::Command;

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
    fn from_str(val: &[String]) -> Option<Result<Box<dyn Command>, String>> {
        match val[0].as_str() {
            "commit" => {
                if val.len() > 1 {
                    Some(Ok(Box::new(CommitCommand::Commit(Some(
                        val[1..].iter().map(|x| x.clone()).collect(),
                    )))))
                } else {
                    Some(Ok(Box::new(CommitCommand::Commit(None))))
                }
            }
            _ => return None,
        }
    }
}

impl AsCommandInfo for CommitCommand {
    fn infos() -> Vec<CommandInfo> {
        vec![CommandInfo::new("commit", [("command", "command")])]
    }
}

#[derive(Clone, Debug, Command)]
#[command(rename_all = "snake_case")]
pub enum BufferCommand {
    MoveCursor { cols: isize, rows: isize },

    WriteFile { path: Option<String> },

    StartChange,
    CommitChange,

    InsertChar(char),
    DeleteChars { offset: isize, count: usize },

    JoinLine(isize),
    InsertNewline(isize),

    InsertLine(isize),
    DeleteLine(isize),

    Undo,
    Redo,
}

impl Command for BufferCommand {
    fn apply(&self, state: std::sync::Arc<crate::State>) -> bool {
        let buffers = state.buffers.read().unwrap();

        let cur_buffer = buffers.cur_buffer();
        let mut cur_buffer = cur_buffer.write().unwrap();

        match self {
            BufferCommand::MoveCursor { rows, cols } => cur_buffer.move_cursor(*rows, *cols),

            BufferCommand::WriteFile { path } => {
                cur_buffer.write_file(path.clone());
                true
            }

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
                    col: col.saturating_add_signed(*offset),
                    len: *count,
                })
            }

            BufferCommand::JoinLine(offset) => {
                let row = cur_buffer.row.saturating_add_signed(*offset);

                cur_buffer.action(JoinLine {
                    row,
                    undo_indent: None,
                })
            }
            BufferCommand::InsertNewline(offset) => {
                let row = cur_buffer.row;
                let col = cur_buffer.col.saturating_add_signed(*offset);

                cur_buffer.action(InsertNewline { row, col })
            }

            BufferCommand::InsertLine(offset) => {
                let row = cur_buffer.row.saturating_add_signed(*offset);

                cur_buffer.action(InsertLine {
                    row,
                    content: "".to_string(),
                    was_last_line: false,
                })
            }
            BufferCommand::DeleteLine(offset) => {
                let row = cur_buffer.row.saturating_add_signed(*offset);

                cur_buffer.action(DeleteLine { row })
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
