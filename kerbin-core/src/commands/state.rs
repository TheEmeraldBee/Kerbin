use crate::*;
use kerbin_macros::Command;
use kerbin_state_machine::State;

use crate::Command;

#[derive(Debug, Clone, Command)]
pub enum StateCommand {
    #[command(name = "q")]
    /// Quits the editor
    Quit,
}

impl Command for StateCommand {
    fn apply(&self, state: &mut State) -> bool {
        match *self {
            Self::Quit => state.lock_state::<Running>().unwrap().0 = false,
        }

        // Always return false, as this command should never be repeated
        false
    }
}
