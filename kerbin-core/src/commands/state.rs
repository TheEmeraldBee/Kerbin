use std::sync::atomic::Ordering;

use crate::*;
use kerbin_macros::Command;

use crate::Command;

#[derive(Debug, Clone, Command)]
pub enum StateCommand {
    #[command(name = "q")]
    Quit,
}

impl Command for StateCommand {
    fn apply(&self, state: std::sync::Arc<crate::State>) -> bool {
        match *self {
            Self::Quit => state.running.store(false, Ordering::Relaxed),
        }

        // Always return false, as this command should never be repeated
        false
    }
}
