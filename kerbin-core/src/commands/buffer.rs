use std::io::ErrorKind;

use kerbin_macros::Command;
use kerbin_state_machine::State;

use crate::*;

const SCRATCH_BUFFER_PATH: &str = "<scratch>";

#[derive(Clone, Debug, Command)]
pub enum CommitCommand {
    /// Commits the command after it as a change
    /// Useful for single commands that should always instacommit
    Commit(#[command(name = "cmd", type_name = "[command]?")] Option<Vec<Token>>),
}

#[async_trait::async_trait]
impl Command for CommitCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            CommitCommand::Commit(after) => {
                let mut res = true;

                state
                    .lock_state::<Buffers>()
                    .await
                    .cur_buffer_mut()
                    .await
                    .start_change_group();

                if let Some(after) = after {
                    let command = state.lock_state::<CommandRegistry>().await.parse_command(
                        after.clone(),
                        true,
                        false,
                        Some(&resolver_engine().await.as_resolver()),
                        true,
                        &*state.lock_state::<CommandPrefixRegistry>().await,
                        &*state.lock_state::<ModeStack>().await,
                    );

                    if let Some(command) = command {
                        res = command.apply(state).await;
                    }
                }

                state
                    .lock_state::<Buffers>()
                    .await
                    .cur_buffer_mut()
                    .await
                    .commit_change_group();

                res
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
        #[command(flag)]
        extend: bool,
    },

    #[command(name = "ml")]
    /// Moves primary cursor by a given number of lines
    MoveLines {
        lines: isize,
        #[command(flag)]
        extend: bool,
    },

    #[command(name = "mc")]
    /// Moves primary cursor by a given number of characters
    MoveChars {
        chars: isize,
        #[command(flag)]
        extend: bool,
    },

    #[command(drop_ident, name = "goto")]
    /// Navigates to the given row and column
    /// Clamps both values to the max positions
    GoTo {
        col: usize,
        row: usize,

        #[command(flag)]
        extend: bool,
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
    Append {
        text: String,
        #[command(flag)]
        extend: bool,
    },

    #[command(name = "del", name = "d")]
    /// Deletes the primary cursor's selection
    Delete,

    #[command(name = "u")]
    /// Reverts the last change, pushing to (and clearing) the redo stack
    Undo,

    #[command(name = "r")]
    /// Reverts the last undo, pushing to (and clearing) the undo stack
    Redo,

    #[command(name = "tgl_case")]
    /// Toggles the case of all characters in selection
    ToggleCase,

    #[command(name = "jl")]
    /// Joins the current line with the next by replacing the trailing newline with a space
    JoinLine,
}

#[async_trait::async_trait]
impl Command for BufferCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut buffers = state.lock_state::<Buffers>().await;

        let log = state.lock_state::<LogSender>().await;

        let mut cur_buffer = buffers.cur_buffer_mut().await;

        let byte = cur_buffer.primary_cursor().get_cursor_byte();

        match self {
            BufferCommand::MoveBytes { bytes, extend } => cur_buffer.move_bytes(*bytes, *extend),
            BufferCommand::MoveLines { lines, extend } => cur_buffer.move_lines(*lines, *extend),
            BufferCommand::MoveChars { chars, extend } => cur_buffer.move_chars(*chars, *extend),
            BufferCommand::GoTo { row, col, extend } => {
                let line_byte = cur_buffer.line_to_byte_clamped(*row);
                let target_byte = line_byte.saturating_add(*col).min(cur_buffer.len());
                let cursor_mut = cur_buffer.primary_cursor_mut();
                if *extend {
                    let anchor_byte = if cursor_mut.at_start {
                        *cursor_mut.sel.end()
                    } else {
                        *cursor_mut.sel.start()
                    };
                    let start = anchor_byte.min(target_byte);
                    let end = anchor_byte.max(target_byte);
                    cursor_mut.set_sel(start..=end);
                    cursor_mut.set_at_start(target_byte < anchor_byte);
                } else {
                    cursor_mut.set_sel(target_byte..=target_byte);
                    cursor_mut.set_at_start(false);
                }

                // This can't be repeated anyways
                false
            }

            BufferCommand::WriteFile { path } => {
                let current_path = if let Some(new_path) = path {
                    new_path.clone()
                } else {
                    cur_buffer.path.clone()
                };

                if current_path.starts_with("<") && current_path.ends_with(">") {
                    log.high(
                        "command::write_file",
                        "Cannot write special file without setting new path",
                    );
                    return false;
                }

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
                        return false;
                    }
                    _ => {}
                }

                if let Err(e) = cur_buffer.write_file(path.clone()).await {
                    log.high("command::write_file", e.to_string());
                    return false;
                }
                true
            }

            BufferCommand::WriteFileForce { path } => {
                if let Err(e) = cur_buffer.write_file(path.clone()).await {
                    log.high("command::write_file", e.to_string());
                    return false;
                }
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

                reload_file_inner(&mut cur_buffer, &log, false)
            }

            BufferCommand::ReloadFileForce => reload_file_inner(&mut cur_buffer, &log, true),

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

            BufferCommand::Append { text, extend } => {
                cur_buffer.action(Insert {
                    byte,
                    content: text.clone(),
                });
                cur_buffer.move_chars(text.chars().count() as isize, *extend)
            }

            BufferCommand::Undo => {
                cur_buffer.undo();
                true
            }
            BufferCommand::Redo => {
                cur_buffer.redo();
                true
            }

            BufferCommand::ToggleCase => {
                let sel = cur_buffer.primary_cursor().sel().clone();
                let sel_start = *sel.start();
                let sel_end = *sel.end();

                let text = cur_buffer
                    .slice_to_string(sel_start, sel_end + 1)
                    .unwrap_or_default();

                let toggled: String = text
                    .chars()
                    .map(|c| {
                        if c.is_uppercase() {
                            c.to_lowercase().next().unwrap_or(c)
                        } else if c.is_lowercase() {
                            c.to_uppercase().next().unwrap_or(c)
                        } else {
                            c
                        }
                    })
                    .collect();

                if text != toggled {
                    let char_count = text.chars().count();
                    cur_buffer.action(Delete {
                        byte: sel_start,
                        len: char_count,
                    });
                    cur_buffer.action(Insert {
                        byte: sel_start,
                        content: toggled,
                    });
                }

                true
            }

            BufferCommand::JoinLine => {
                let line_idx = cur_buffer.byte_to_line_clamped(byte);
                if line_idx + 1 >= cur_buffer.len_lines() {
                    return false;
                }
                let next_line_start = cur_buffer.line_to_byte_clamped(line_idx + 1);
                let newline_byte = next_line_start.saturating_sub(1);
                if newline_byte < next_line_start {
                    cur_buffer.action(Delete {
                        byte: newline_byte,
                        len: 1,
                    });
                    cur_buffer.action(Insert {
                        byte: newline_byte,
                        content: " ".to_string(),
                    });
                }
                true
            }

            BufferCommand::Delete => {
                let range = cur_buffer.primary_cursor().sel().clone();
                cur_buffer.primary_cursor_mut().set_at_start(true);
                cur_buffer.primary_cursor_mut().collapse_sel();

                let start = *range.start();
                let end = *range.end();

                let char_idx_start = cur_buffer.byte_to_char_clamped(start);
                let char_idx_end = cur_buffer.byte_to_char_clamped(end);

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

fn reload_file_inner(buf: &mut TextBuffer, log: &LogSender, force: bool) -> bool {
    if !force && buf.dirty {
        let message = "Cannot reload file: buffer has unsaved changes. Use reload! to force.";
        tracing::error!(message);
        log.medium("command::reload_file", message);
        return false;
    }

    let path = buf.path.clone();
    if path == SCRATCH_BUFFER_PATH {
        let message = "Cannot reload scratch buffer";
        tracing::error!(message);
        log.medium("command::reload_file", message);
        return false;
    }

    match std::fs::File::open(&path) {
        Ok(f) => match ropey::Rope::from_reader(std::io::BufReader::new(f)) {
            Ok(rope) => {
                buf.rope = rope;
                buf.dirty = false;
                buf.undo_stack.clear();
                buf.redo_stack.clear();
                buf.save_point = 0;

                if let Ok(metadata) = std::fs::metadata(&path) {
                    buf.changed = metadata.modified().ok();
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
        let mut buffers = state.lock_state::<Buffers>().await;
        let log = state.lock_state::<LogSender>().await;
        let default_tab_unit = state.lock_state::<CoreConfig>().await.default_tab_unit;

        match self {
            Self::OpenFile(path) => {
                let buffer_id = match buffers.open(path.clone(), default_tab_unit).await {
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

                // Track the opened buffer in the focused pane
                let mut split = state.lock_state::<SplitState>().await;
                if !split.unique_buffers {
                    if let Some(pane) = split.focused_pane_mut() {
                        pane.selected_local = buffer_id;
                    }
                } else if let Some(pane) = split.focused_pane_mut() {
                    if !pane.buffer_indices.contains(&buffer_id) {
                        pane.buffer_indices.push(buffer_id);
                    }
                    pane.selected_local = pane
                        .buffer_indices
                        .iter()
                        .position(|&x| x == buffer_id)
                        .unwrap_or(0);
                }

                true
            }

            Self::SwitchBuffer(offset) => {
                let mut split = state.lock_state::<SplitState>().await;
                if !split.unique_buffers {
                    let n = buffers.buffers.len();
                    if n == 0 {
                        return false;
                    }
                    if let Some(pane) = split.focused_pane_mut() {
                        pane.selected_local =
                            (pane.selected_local as isize + offset).rem_euclid(n as isize) as usize;
                        let new_idx = pane.selected_local;
                        buffers.set_selected_buffer(new_idx);
                    }
                } else if let Some(pane) = split.focused_pane_mut() {
                    if pane.buffer_indices.is_empty() {
                        return false;
                    }
                    let n = pane.buffer_indices.len() as isize;
                    let new_local =
                        (pane.selected_local as isize + offset).rem_euclid(n) as usize;
                    pane.selected_local = new_local;
                    let new_buf_idx = pane.buffer_indices[new_local];
                    buffers.set_selected_buffer(new_buf_idx);
                }
                true
            }

            Self::CloseBufferOffset(offset) => {
                close_buffer_inner(state, &mut buffers, &log, offset.unwrap_or(0), false).await
            }

            Self::CloseBufferOffsetForce(offset) => {
                close_buffer_inner(state, &mut buffers, &log, offset.unwrap_or(0), true).await
            }
        }
    }
}

async fn close_buffer_inner(
    state: &State,
    buffers: &mut Buffers,
    log: &LogSender,
    offset: isize,
    force: bool,
) -> bool {
    let buf_idx = buffers.selected_buffer as isize + offset;
    if buf_idx >= buffers.buffers.len() as isize || buf_idx < 0 {
        return false;
    }
    let buf_idx = buf_idx as usize;

    if !force {
        let dirty = buffers.buffers[buf_idx].read().await.dirty;
        if dirty {
            log.medium("command::close_buffer", "Cannot close buffer as it has changes!");
            tracing::error!("Cannot close buffer as it has changes!");
            return false;
        }
    }

    let shared = !state.lock_state::<SplitState>().await.unique_buffers;
    if shared {
        buffers.close_buffer(buf_idx).await;
        fixup_shared_panes_after_close(state, buffers, buf_idx).await;
    } else {
        let (close_globally, new_sel) = {
            let mut split = state.lock_state::<SplitState>().await;
            if let Some(pane) = split.focused_pane_mut() {
                pane.buffer_indices.retain(|&x| x != buf_idx);
                if pane.selected_local >= pane.buffer_indices.len() {
                    pane.selected_local = pane.buffer_indices.len().saturating_sub(1);
                }
            }
            let referenced = split.leaves().iter().any(|p| p.buffer_indices.contains(&buf_idx));
            let sel = split
                .focused_pane()
                .and_then(|p| p.buffer_indices.get(p.selected_local).copied());
            (!referenced, sel)
        };
        if close_globally {
            buffers.close_buffer(buf_idx).await;
            let mut split = state.lock_state::<SplitState>().await;
            for pane in split.leaves_mut() {
                for idx in &mut pane.buffer_indices {
                    if *idx > buf_idx {
                        *idx -= 1;
                    }
                }
            }
            let re_sel = split
                .focused_pane()
                .and_then(|p| p.buffer_indices.get(p.selected_local).copied());
            drop(split);
            if let Some(idx) = re_sel {
                buffers.set_selected_buffer(idx);
            }
        } else if let Some(idx) = new_sel {
            buffers.set_selected_buffer(idx);
        }
    }
    true
}

/// After closing a buffer at `buf_idx` in shared mode, adjusts every pane's
/// `selected_local` so it remains a valid global buffer index, then syncs
/// `Buffers.selected_buffer` to the focused pane.
async fn fixup_shared_panes_after_close(state: &State, buffers: &mut Buffers, buf_idx: usize) {
    let new_len = buffers.buffers.len();
    let mut split = state.lock_state::<SplitState>().await;
    for pane in split.leaves_mut() {
        if pane.selected_local > buf_idx {
            pane.selected_local -= 1;
        }
        if new_len > 0 && pane.selected_local >= new_len {
            pane.selected_local = new_len - 1;
        }
    }
    let new_sel = split.focused_pane().map(|p| p.selected_local);
    drop(split);
    if let Some(sel) = new_sel {
        buffers.set_selected_buffer(sel);
    }
}
