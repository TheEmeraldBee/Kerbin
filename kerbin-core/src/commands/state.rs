use std::sync::atomic::Ordering;

use serde::Deserialize;

use crate::Command;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateCommand {
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
