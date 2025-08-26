use kerbin_macros::Command;

use crate::*;

pub use ropey::LineType;

fn parse_commit(val: &[String]) -> Result<Box<dyn Command>, String> {
    if val.len() > 1 {
        Ok(Box::new(CommitCommand::Commit(Some(val[1..].to_vec()))))
    } else {
        Ok(Box::new(CommitCommand::Commit(None)))
    }
}

#[derive(Clone, Debug, Command)]
pub enum CommitCommand {
    #[command(parser = "parse_commit")]
    Commit(#[command(name = "cmd", type_name = "command")] Option<Vec<String>>),
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

#[derive(Clone, Debug, Command)]
pub enum BufferCommand {
    #[command(name = "mc")]
    MoveCursor {
        cols: isize,
        rows: isize,

        #[command(type_name = "bool?")]
        extend: Option<bool>,
    },

    #[command(name = "write", name = "w")]
    WriteFile {
        #[command(type_name = "string?")]
        path: Option<String>,
    },

    StartChange,
    CommitChange,

    #[command(name = "ins", name = "i")]
    InsertChar(char),

    #[command(name = "del", name = "d")]
    Delete,

    #[command(name = "u")]
    Undo,

    #[command(name = "r")]
    Redo,
}

impl Command for BufferCommand {
    fn apply(&self, state: std::sync::Arc<crate::State>) -> bool {
        let buffers = state.buffers.read().unwrap();

        let cur_buffer = buffers.cur_buffer();
        let mut cur_buffer = cur_buffer.write().unwrap();

        let byte = cur_buffer.primary_cursor().get_cursor_byte();

        match self {
            BufferCommand::MoveCursor { rows, cols, extend } => {
                cur_buffer.move_cursor(*rows, *cols, extend.unwrap_or(false))
            }

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

            BufferCommand::InsertChar(chr) => cur_buffer.action(Insert {
                byte,
                content: chr.to_string(),
            }),

            BufferCommand::Undo => {
                cur_buffer.undo();
                true
            }
            BufferCommand::Redo => {
                cur_buffer.redo();
                true
            }

            BufferCommand::Delete => {
                let range = cur_buffer.primary_cursor().sel().clone();
                cur_buffer.primary_cursor_mut().collapse_sel();

                let len = range.end() - range.start();
                if len > 0 {
                    cur_buffer.action(Delete {
                        byte: *range.start(),
                        len,
                    })
                } else {
                    true
                }
            }
        }
    }
}

#[derive(Debug, Clone, Command)]
pub enum BuffersCommand {
    #[command(name = "open", name = "o")]
    OpenFile(String),

    #[command(drop_ident, name = "move_buf", name = "bm")]
    SwitchBuffer(isize),

    #[command(drop_ident, name = "buf_close", name = "bc")]
    CloseBufferOffset(Option<isize>),
}

impl Command for BuffersCommand {
    fn apply(&self, state: std::sync::Arc<State>) -> bool {
        let mut buffers = state.buffers.write().unwrap();

        match self {
            Self::OpenFile(path) => {
                let buffer_id = buffers.open(
                    path.clone(),
                    &mut state.grammar.write().unwrap(),
                    &state.theme.read().unwrap(),
                );
                buffers.set_selected_buffer(buffer_id);
                true
            }

            Self::SwitchBuffer(offset) => {
                buffers.change_buffer(*offset);
                true
            }

            Self::CloseBufferOffset(offset) => {
                let offset = offset.unwrap_or(0);
                let buf_idx = buffers.selected_buffer as isize + offset;

                if buf_idx >= buffers.buffers.len() as isize || buf_idx < 0 {
                    return false;
                }

                buffers.close_buffer(buf_idx as usize);
                true
            }
        }
    }
}
