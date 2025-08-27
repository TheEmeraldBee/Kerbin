use kerbin_macros::Command;

use crate::*;

#[derive(Debug, Clone, Command)]
pub enum ModeCommand {
    #[command(name = "cm")]
    ChangeMode(char),

    #[command(name = "pm")]
    PushMode(char),

    #[command(name = "rm")]
    PopMode,
}

impl Command for ModeCommand {
    fn apply(&self, state: std::sync::Arc<crate::State>) -> bool {
        match *self {
            ModeCommand::ChangeMode(new) => state.set_mode(new),
            ModeCommand::PushMode(new) => state.push_mode(new),
            ModeCommand::PopMode => {
                state.pop_mode();
            }
        }

        // Always return false, as this should never be repeated
        false
    }
}
