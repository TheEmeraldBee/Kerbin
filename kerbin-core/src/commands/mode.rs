use kerbin_macros::Command;

use crate::*;

#[derive(Debug, Clone, Command)]
#[command(rename_all = "snake_case")]
pub enum ModeCommand {
    ChangeMode(char),
}

impl Command for ModeCommand {
    fn apply(&self, state: std::sync::Arc<crate::State>) -> bool {
        match *self {
            ModeCommand::ChangeMode(new) => state.set_mode(new),
        }

        // Always return false, as this should never be repeated
        false
    }
}
