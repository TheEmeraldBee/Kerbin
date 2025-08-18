use std::sync::Arc;

use crate::State;

pub mod state;
pub use state::*;

pub mod buffer;
pub use buffer::*;

pub mod mode;
pub use mode::*;

use serde::de::DeserializeOwned;

pub trait Command: Send + Sync {
    fn apply(&self, state: Arc<State>) -> bool;
}

/// This trait should be implemented on anything you want to be able to define within a config
/// This will turn the command into an executable command based on the string input.
/// Used for config, as well as the command pallette Serde + a parsing library can make this much
/// easier to implement
pub trait CommandFromStr: Send + Sync {
    fn from_str(val: &[String]) -> Option<Box<dyn Command>>;
}

impl<T: Command + DeserializeOwned + 'static> CommandFromStr for T {
    fn from_str(val: &[String]) -> Option<Box<dyn Command>> {
        match kerbin_serde::from_slice::<Self>(val) {
            Ok(t) => Some(Box::new(t)),
            Err(_) => None,
        }
    }
}
