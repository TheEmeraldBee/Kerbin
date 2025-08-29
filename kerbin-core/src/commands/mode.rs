use kerbin_macros::Command;

use crate::*;

#[derive(Debug, Clone, Command)]
pub enum ModeCommand {
    #[command(name = "cm")]
    /// Clears the mode stack and sets it to the given char
    /// Should almost never be used, please use "pm" and "rm" instead
    ChangeMode(char),

    #[command(name = "pm")]
    /// Pushes a new mode to the mode stack
    PushMode(char),

    #[command(name = "rm")]
    /// Pops the current mode from the mode stack, limited at the base ('n')
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
