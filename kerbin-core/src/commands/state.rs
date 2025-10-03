use crate::*;
use kerbin_macros::Command;
use kerbin_state_machine::State;

use crate::Command;

#[derive(Debug, Clone, Command)]
pub enum StateCommand {
    #[command(name = "q")]
    /// Quits the editor, respecting the dirty flag
    /// see `quit!` for a command that ignores the flag
    Quit,

    #[command(drop_ident, name = "quit!", name = "q!")]
    /// Quits the editor, ignoring the dirty flag
    /// see `quit` for a command that respects flags
    QuitForce,

    #[command(drop_ident, name = "log_session")]
    LogSessionId,
}

impl Command for StateCommand {
    fn apply(&self, state: &mut State) -> bool {
        match *self {
            Self::Quit => {
                let bufs = state.lock_state::<Buffers>().unwrap();
                let log = state.lock_state::<LogSender>().unwrap();
                for buf in &bufs.buffers {
                    if buf.read().unwrap().dirty {
                        log.medium(
                            "command::quit",
                            "Unable to quit, can't close unsaved buffers",
                        );

                        tracing::error!("Unable to quit project, please save buffers");
                        return false;
                    }
                }
                state.lock_state::<Running>().unwrap().0 = false;
            }
            Self::QuitForce => {
                state.lock_state::<Running>().unwrap().0 = false;
            }

            Self::LogSessionId => {
                let session_uuid = state.lock_state::<SessionUuid>().unwrap().0;
                state
                    .lock_state::<LogSender>()
                    .unwrap()
                    .high("command::log_session", session_uuid);
            }
        }

        // Always return false, as this command should never be repeated
        false
    }
}
