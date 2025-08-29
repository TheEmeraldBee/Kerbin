use kerbin_macros::Command;

use crate::*;

#[derive(Clone, Command)]
pub enum CursorCommand {
    #[command(name = "cc")]
    /// Duplicates the primary cursor
    /// Should always run 2 commands, one to do something, then one to move the cursor
    /// Else the cursor will get cleared immediately after
    /// Also sets the primary cursor to the new one
    CreateCursor,

    #[command(name = "cac")]
    /// Moves the active cursor by an offset
    ChangeActiveCursor(#[command(name = "offset")] isize),

    #[command(name = "dc")]
    /// Will drop the primary cursor (assuming there are more than one cursor)
    DropCursor,

    #[command(name = "dcs")]
    /// Will clear all cursors other than the primary cursor
    DropOtherCursors,

    #[command(
        drop_ident,
        name = "apply_all_cursor",
        name = "aa",
        parser = "parse_apply_all"
    )]
    /// Applies the following command to all cursors
    /// Emulates a true multicursor environment
    ApplyAll(#[command(name = "cmd", type_name = "command")] Vec<String>),
}

fn parse_apply_all(val: &[String]) -> Result<Box<dyn Command>, String> {
    if val.len() == 1 {
        return Err("Expected at least 1 argument".to_string());
    }
    Ok(Box::new(CursorCommand::ApplyAll(val[1..].to_vec())))
}

impl Command for CursorCommand {
    fn apply(&self, state: std::sync::Arc<State>) -> bool {
        let cur_buf = state.buffers.read().unwrap().cur_buffer();
        let mut cur_buf = cur_buf.write().unwrap();

        match self {
            Self::CreateCursor => {
                cur_buf.create_cursor();
                true
            }

            Self::ChangeActiveCursor(offset) => {
                cur_buf.change_cursor(*offset);
                true
            }

            Self::DropCursor => {
                cur_buf.drop_primary_cursor();
                true
            }

            Self::DropOtherCursors => {
                cur_buf.drop_other_cursors();
                true
            }

            Self::ApplyAll(cmd) => {
                let primary_cursor = cur_buf.primary_cursor;
                let cursor_count = cur_buf.cursors.len();

                let mut res = true;
                drop(cur_buf);
                for i in 0..cursor_count {
                    state
                        .buffers
                        .read()
                        .unwrap()
                        .cur_buffer()
                        .write()
                        .unwrap()
                        .primary_cursor = i;

                    match state.parse_command(cmd.clone(), true, true) {
                        Some(t) => {
                            t.apply(state.clone());
                        }
                        None => {
                            res = false;
                            break;
                        }
                    };
                }

                state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .write()
                    .unwrap()
                    .primary_cursor = primary_cursor;

                res
            }
        }
    }
}
