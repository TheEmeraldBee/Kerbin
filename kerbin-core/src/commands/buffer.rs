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
    /// Commits the command after it as a change
    /// Useful for single commands that should always instacommit
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
    /// Moves primary cursor
    ///
    /// If extended is true, will extend selection instead of replacing it
    #[command(name = "mc")]
    MoveCursor {
        cols: isize,
        rows: isize,

        #[command(type_name = "bool?")]
        extend: Option<bool>,
    },

    #[command(name = "write", name = "w")]
    /// Writes the given file to disk
    /// An optional path can be given to write the file to a different name
    /// Will not write if filename is <scratch>
    WriteFile {
        #[command(type_name = "string?")]
        path: Option<String>,
    },

    /// Starts a comittable change (allows for undo and redo)
    StartChange,
    /// Commits the active change (does nothing if no change is active)
    CommitChange,

    #[command(name = "ins", name = "i")]
    /// Inserts the given content at the primary cursor's location
    Insert(String),

    #[command(name = "apnd", name = "a")]
    /// Appends the given content at the primary cursor's location, extending the selection if set
    Append(String, #[command(name = "extend")] bool),

    #[command(name = "del", name = "d")]
    /// Deletes the primary cursor's selection
    Delete,

    #[command(name = "u")]
    /// Reverts the last change, pushing to (and clearing) the redo stack
    Undo,

    #[command(name = "r")]
    /// Reverts the last undo, pushing to (and clearing) the undo stack
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

            BufferCommand::Insert(text) => cur_buffer.action(Insert {
                byte,
                content: text.clone(),
            }),

            BufferCommand::Append(text, extend) => {
                cur_buffer.action(Insert {
                    byte,
                    content: text.clone(),
                });
                cur_buffer.move_cursor(0, text.len() as isize, *extend)
            }

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

                let start = *range.start();
                let len = range.count();
                if len > 0 {
                    cur_buffer.action(Delete { byte: start, len })
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
    /// Opens the given filepath can be absolute or relative
    OpenFile(String),

    #[command(drop_ident, name = "move_buf", name = "bm")]
    /// Moves the currently active buffer based on an offset
    SwitchBuffer(isize),

    #[command(drop_ident, name = "buf_close", name = "bc")]
    /// Closes the current buffer unless an offset is passed
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
