use crate::*;

#[derive(Clone, Command)]
pub enum CursorCommand {
    #[command(name = "cc")]
    /// Duplicates the primary cursor and sets it as the new primary.
    ///
    /// Follow this with a cursor-movement command; the duplicated cursor is cleared
    /// if its position is not moved before the next frame.
    CreateCursor,

    #[command(name = "cac")]
    /// Moves the active cursor by an offset.
    ChangeActiveCursor(#[command(name = "offset")] isize),

    #[command(name = "dc")]
    /// Drops the primary cursor (no-op if it is the only cursor).
    DropCursor,

    #[command(name = "dcs")]
    /// Clears all cursors except the primary cursor.
    DropOtherCursors,

    #[command(drop_ident, name = "apply_all_cursor", name = "aa")]
    /// Applies a command to every cursor, emulating a true multicursor environment.
    ApplyAll(#[command(name = "cmd", type_name = "[command]", ignore)] Vec<Token>),
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
