use std::sync::Arc;

use crate::State;

pub mod state;
pub use state::*;

pub mod buffer;
pub use buffer::*;

pub mod mode;
pub use mode::*;

pub trait Command: Send + Sync {
    fn apply(&self, state: Arc<State>) -> bool;
}

pub struct CommandInfo {
    pub name: String,
    pub args: Vec<(String, String)>,
}

impl CommandInfo {
    pub fn new(
        name: impl ToString,
        args: impl IntoIterator<Item = (impl ToString, impl ToString)>,
    ) -> Self {
        Self {
            name: name.to_string(),
            args: args
                .into_iter()
                .map(|x| (x.0.to_string(), x.1.to_string()))
                .collect(),
        }
    }
}

/// This trait will allow you to use commands from 'c' mode. This will give you verification info,
/// as well as argument expectations and types. This shouldn't need to be implemented manually.
/// Just use the #[derive(Command)] and the additional attributes on the struct.
pub trait AsCommandInfo: Command + CommandFromStr {
    fn infos() -> Vec<CommandInfo>;
}

/// This trait should be implemented on anything you want to be able to define within a config
/// This will turn the command into an executable command based on the string input.
/// Used for config, as well as the command pallette Serde + a parsing library can make this much
/// easier to implement
pub trait CommandFromStr: Command {
    fn from_str(val: &[String]) -> Option<Result<Box<dyn Command>, String>>;
}
