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

    #[command(drop_ident, name = "apply_all_cursor", name = "aa")]
    /// Applies the following command to all cursors
    /// Emulates a true multicursor environment
    ApplyAll(#[command(name = "cmd", type_name = "[command]")] Vec<Token>),
}

#[async_trait::async_trait]
impl Command<State> for CursorCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut cur_bufs = state.lock_state::<Buffers>().await;

        match self {
            Self::CreateCursor => {
                let Some(mut tb) = cur_bufs.cur_text_buffer_mut().await else {
                    return false;
                };
                tb.create_cursor();
                true
            }

            Self::ChangeActiveCursor(offset) => {
                let Some(mut tb) = cur_bufs.cur_text_buffer_mut().await else {
                    return false;
                };
                tb.change_cursor(*offset);
                true
            }

            Self::DropCursor => {
                let Some(mut tb) = cur_bufs.cur_text_buffer_mut().await else {
                    return false;
                };
                tb.drop_primary_cursor();
                true
            }

            Self::DropOtherCursors => {
                let Some(mut tb) = cur_bufs.cur_text_buffer_mut().await else {
                    return false;
                };
                tb.drop_other_cursors();
                true
            }

            Self::ApplyAll(cmd) => {
                let (primary_cursor, cursor_count) = {
                    match cur_bufs.cur_buffer_as::<TextBuffer>().await {
                        Some(tb) => (tb.primary_cursor, tb.cursors.len()),
                        None => return false,
                    }
                };

                let command = state.lock_state::<CommandRegistry>().await.parse_command(
                    cmd.clone(),
                    true,
                    true,
                    Some(&resolver_engine().await.as_resolver()),
                    true,
                    &*state.lock_state::<CommandPrefixRegistry>().await,
                    &*state.lock_state::<ModeStack>().await,
                );

                let Some(command) = command else {
                    return false;
                };

                // Drop lock to prevent deadlock
                drop(cur_bufs);

                for i in 0..cursor_count {
                    {
                        let mut guard = state.lock_state::<Buffers>().await;
                        if let Some(mut tb) = guard.cur_text_buffer_mut().await {
                            tb.primary_cursor = i;
                        }
                    }

                    dispatch_command(command.as_ref(), state).await;

                    {
                        let guard = state.lock_state::<Buffers>().await;
                        let count = guard
                            .cur_buffer_as::<TextBuffer>()
                            .await
                            .map(|tb| tb.cursors.len())
                            .unwrap_or(0);
                        if count != cursor_count {
                            tracing::error!(
                                "Apply All Ran command that changed cursor count. This is not allowed at current time."
                            );
                            break;
                        }
                    }
                }

                {
                    let mut guard = state.lock_state::<Buffers>().await;
                    if let Some(mut tb) = guard.cur_text_buffer_mut().await {
                        tb.primary_cursor = primary_cursor;
                    }
                }

                false
            }
        }
    }
}
