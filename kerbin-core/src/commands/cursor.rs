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

#[async_trait::async_trait]
impl Command for CursorCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut cur_buf = state.lock_state::<Buffers>().await.cur_buffer_mut().await;

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

                let command = state.lock_state::<CommandRegistry>().await.parse_command(
                    cmd.clone(),
                    true,
                    true,
                    &*state.lock_state::<CommandPrefixRegistry>().await,
                    &*state.lock_state::<ModeStack>().await,
                );

                let Some(command) = command else {
                    return false;
                };

                // Drop lock to prevent deadlock
                drop(cur_buf);

                for i in 0..cursor_count {
                    state
                        .lock_state::<Buffers>()
                        .await
                        .cur_buffer_mut()
                        .await
                        .primary_cursor = i;

                    command.apply(state).await;

                    if state
                        .lock_state::<Buffers>()
                        .await
                        .cur_buffer_mut()
                        .await
                        .cursors
                        .len()
                        != cursor_count
                    {
                        // This is a fail state, log and break
                        tracing::error!(
                            "Apply All Ran command that changed cursor count. This is not allowed at current time."
                        );
                        break;
                    }
                }

                state
                    .lock_state::<Buffers>()
                    .await
                    .cur_buffer_mut()
                    .await
                    .primary_cursor = primary_cursor;

                false
            }
        }
    }
}
