use std::io::ErrorKind;

use kerbin_macros::Command;
use kerbin_state_machine::State;

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

#[async_trait::async_trait]
impl Command for CommitCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            CommitCommand::Commit(after) => {
                // Begin the change
                state
                    .lock_state::<Buffers>()
                    .await
                    .unwrap()
                    .cur_buffer_mut()
                    .await
                    .start_change_group();

                if let Some(after) = after {
                    let command = state
                        .lock_state::<CommandRegistry>()
                        .await
                        .unwrap()
                        .parse_command(
                            after.clone(),
                            true,
                            false,
                            &state.lock_state::<CommandPrefixRegistry>().await.unwrap(),
                            &state.lock_state::<ModeStack>().await.unwrap(),
                        );
                    if let Some(command) = command {
                        state
                            .lock_state::<CommandSender>()
                            .await
                            .unwrap()
                            .send(command)
                            .unwrap();
                    }
                }

                // End the change
                state
                    .lock_state::<Buffers>()
                    .await
                    .unwrap()
                    .cur_buffer_mut()
                    .await
                    .commit_change_group();

                true
            }
        }
    }
}

#[derive(Clone, Debug, Command)]
pub enum BufferCommand {
    #[command(name = "mb")]
    /// Moves primary cursor by a given number of bytes
    MoveBytes {
        bytes: isize,
        #[command(type_name = "bool?")]
        extend: Option<bool>,
    },

    #[command(name = "ml")]
    /// Moves primary cursor by a given number of lines
    MoveLines {
        lines: isize,
        #[command(type_name = "bool?")]
        extend: Option<bool>,
    },

    #[command(name = "mc")]
    /// Moves primary cursor by a given number of characters
    MoveChars {
        chars: isize,
        #[command(type_name = "bool?")]
        extend: Option<bool>,
    },

    #[command(name = "write", name = "w")]
    /// Writes the given file to disk
    /// An optional path can be given to write the file to a different name
    /// Will not write if filename is <scratch>, or if there are external changes
    ///
    /// In order to not respect external changes, see `write_file!`
    WriteFile {
        #[command(type_name = "string?")]
        path: Option<String>,
    },

    #[command(drop_ident, name = "write_file!", name = "write!", name = "w!")]
    /// Writes the given file to disk, ignoring metadata
    /// if you want to respect external changes, see `write_file`
    WriteFileForce {
        #[command(type_name = "string?")]
        path: Option<String>,
    },

    #[command(name = "reload", name = "e")]
    /// Reloads the current buffer from disk
    /// Will not reload if the buffer is dirty (has unsaved changes)
    /// For a version that ignores the dirty flag, see `reload_file!`
    ReloadFile,

    #[command(drop_ident, name = "reload!", name = "e!")]
    /// Reloads the current buffer from disk, ignoring the dirty flag
    /// If you want to respect the dirty flag, see `reload_file`
    ReloadFileForce,

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

#[async_trait::async_trait]
impl Command for BufferCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut buffers = state.lock_state::<Buffers>().await.unwrap();

        let log = state.lock_state::<LogSender>().await.unwrap();

        let mut cur_buffer = buffers.cur_buffer_mut().await;

        let byte = cur_buffer.primary_cursor().get_cursor_byte();

        match self {
            BufferCommand::MoveBytes { bytes, extend } => {
                cur_buffer.move_bytes(*bytes, extend.unwrap_or(false))
            }
            BufferCommand::MoveLines { lines, extend } => {
                cur_buffer.move_lines(*lines, extend.unwrap_or(false))
            }
            BufferCommand::MoveChars { chars, extend } => {
                cur_buffer.move_chars(*chars, extend.unwrap_or(false))
            }

            BufferCommand::WriteFile { path } => {
                let current_path = if let Some(new_path) = path {
                    new_path.clone()
                } else {
                    cur_buffer.path.clone()
                };

                if current_path != "<scratch>" {
                    match std::fs::metadata(&current_path) {
                        Ok(metadata) => {
                            let disk_modified = metadata.modified().ok();
                            let buffer_changed = cur_buffer.changed;

                            if let Some(disk_time) = disk_modified
                                && let Some(buffer_time) = buffer_changed
                                && disk_time != buffer_time
                            {
                                let message = format!(
                                    "File has been modified externally since last read/save: {}. Disk time: {:?}, Buffer time: {:?}",
                                    current_path, disk_time, buffer_time
                                );

                                tracing::error!(message);
                                log.high("command::write_file", message);
                                return false;
                            }
                        }
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                            let message =
                                format!("Failed to read metadata for file {}: {}", current_path, e);
                            tracing::error!(message);
                            log.high("command::write_file", message);
                            // Treat as if external file changes if the metadata isn't found
                            return false;
                        }
                        _ => {
                            // File not found on disk, so no conflict check needed.
                        }
                    }
                }

                cur_buffer.write_file(path.clone());
                true
            }

            BufferCommand::WriteFileForce { path } => {
                cur_buffer.write_file(path.clone());
                true
            }

            BufferCommand::ReloadFile => {
                if cur_buffer.dirty {
                    let message =
                        "Cannot reload file: buffer has unsaved changes. Use reload! to force.";
                    tracing::error!(message);
                    log.medium("command::reload_file", message);
                    return false;
                }

                let path = cur_buffer.path.clone();
                if path == "<scratch>" {
                    let message = "Cannot reload scratch buffer";
                    tracing::error!(message);
                    log.medium("command::reload_file", message);
                    return false;
                }

                match std::fs::File::open(&path) {
                    Ok(f) => match ropey::Rope::from_reader(std::io::BufReader::new(f)) {
                        Ok(rope) => {
                            cur_buffer.rope = rope;
                            cur_buffer.dirty = false;
                            cur_buffer.undo_stack.clear();
                            cur_buffer.redo_stack.clear();
                            cur_buffer.save_point = 0;

                            if let Ok(metadata) = std::fs::metadata(&path) {
                                cur_buffer.changed = metadata.modified().ok();
                            }

                            true
                        }
                        Err(e) => {
                            let message = format!("Failed to read file {}: {}", path, e);
                            tracing::error!(message);
                            log.high("command::reload_file", message);
                            false
                        }
                    },
                    Err(e) => {
                        let message = format!("Failed to open file {}: {}", path, e);
                        tracing::error!(message);
                        log.high("command::reload_file", message);
                        false
                    }
                }
            }

            BufferCommand::ReloadFileForce => {
                let path = cur_buffer.path.clone();
                if path == "<scratch>" {
                    let message = "Cannot reload scratch buffer";
                    tracing::error!(message);
                    log.high("command::reload_file", message);
                    return false;
                }

                match std::fs::File::open(&path) {
                    Ok(f) => match ropey::Rope::from_reader(std::io::BufReader::new(f)) {
                        Ok(rope) => {
                            cur_buffer.rope = rope;
                            cur_buffer.dirty = false;
                            cur_buffer.undo_stack.clear();
                            cur_buffer.redo_stack.clear();
                            cur_buffer.save_point = 0;

                            if let Ok(metadata) = std::fs::metadata(&path) {
                                cur_buffer.changed = metadata.modified().ok();
                            }

                            true
                        }
                        Err(e) => {
                            let message = format!("Failed to read file {}: {}", path, e);
                            tracing::error!(message);
                            log.high("command::reload_file", message);
                            false
                        }
                    },
                    Err(e) => {
                        let message = format!("Failed to open file {}: {}", path, e);
                        tracing::error!(message);
                        log.high("command::reload_file", message);
                        false
                    }
                }
            }

            BufferCommand::StartChange => {
                cur_buffer.start_change_group();
                true
            }

            BufferCommand::CommitChange => {
                cur_buffer.commit_change_group();
                true
            }

            BufferCommand::Insert(text) => {
                let processed_text = process_escape_sequences(text);
                cur_buffer.action(Insert {
                    byte,
                    content: processed_text.clone(),
                })
            }

            BufferCommand::Append(text, extend) => {
                let processed_text = process_escape_sequences(text);
                cur_buffer.action(Insert {
                    byte,
                    content: processed_text.clone(),
                });
                cur_buffer.move_chars(processed_text.len() as isize, *extend)
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
                cur_buffer.primary_cursor_mut().set_at_start(true);
                cur_buffer.primary_cursor_mut().collapse_sel();

                let start = *range.start();
                let end = *range.end();

                let char_idx_start = cur_buffer.rope.byte_to_char_idx(start);
                let char_idx_end = cur_buffer.rope.byte_to_char_idx(end);

                let chars_count = char_idx_end + 1 - char_idx_start;

                if chars_count > 0 {
                    cur_buffer.action(Delete {
                        byte: start,
                        len: chars_count,
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
    /// Opens the given filepath can be absolute or relative
    OpenFile(String),

    #[command(drop_ident, name = "move_buf", name = "bm")]
    /// Moves the currently active buffer based on an offset
    SwitchBuffer(isize),

    #[command(drop_ident, name = "buf_close", name = "bc")]
    /// Closes the current buffer unless an offset is passed
    CloseBufferOffset(Option<isize>),

    #[command(drop_ident, name = "buf_close!", name = "bc!")]
    /// Force closes the current buffer unless offset is passed (ignores dirty flag)
    /// for a command that respects the dirty flag, see `buf_close`
    CloseBufferOffsetForce(Option<isize>),
}

#[async_trait::async_trait]
impl Command for BuffersCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut buffers = state.lock_state::<Buffers>().await.unwrap();
        let log = state.lock_state::<LogSender>().await.unwrap();

        match self {
            Self::OpenFile(path) => {
                let buffer_id = match buffers.open(path.clone()).await {
                    Ok(t) => t,
                    Err(e) => {
                        match e.kind() {
                            ErrorKind::NotFound => {
                                log.critical(
                                    "command::open_file",
                                    format!("File '{}' was not found, does it exist?", path),
                                );
                            }
                            ErrorKind::IsADirectory => {
                                log.critical(
                                    "command::open_file",
                                    format!("Expected a file, but '{}' is a directory", path),
                                );
                            }
                            ErrorKind::PermissionDenied => {
                                log.critical(
                                    "command::open_file",
                                    format!("Permission to open file '{}' was denied", path),
                                );
                            }
                            _ => {
                                log.critical("command::open_file", e);
                            }
                        }
                        return false;
                    }
                };
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

                let dirty = buffers.buffers[buf_idx as usize].read().await.dirty;

                if dirty {
                    log.medium(
                        "command::close_buffer",
                        "Cannot close buffer as it has changes!",
                    );
                    tracing::error!("Cannot close buffer as it has changes!");
                    false
                } else {
                    buffers.close_buffer(buf_idx as usize);
                    true
                }
            }

            Self::CloseBufferOffsetForce(offset) => {
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

/// Process escape sequences in a string
fn process_escape_sequences(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    '\\' => result.push('\\'),
                    '\'' => result.push('\''),
                    '"' => result.push('"'),
                    '0' => result.push('\0'),
                    _ => {
                        // Unknown escape, keep as-is
                        result.push('\\');
                        result.push(next);
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }

    result
}
