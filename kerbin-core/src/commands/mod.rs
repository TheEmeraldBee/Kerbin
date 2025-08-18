use std::sync::Arc;

use crate::State;

pub mod buffer;
pub use buffer::*;
use serde::de::DeserializeOwned;

pub trait Command: Send + Sync {
    fn apply(&self, state: Arc<State>) -> bool;
}

pub trait CommandFromStr: Send + Sync {
    fn from_str(val: &str) -> Option<Box<dyn Command>>;
}

impl<T: DeserializeOwned + Command + 'static> CommandFromStr for T {
    fn from_str(val: &str) -> Option<Box<dyn Command>> {
        match toml::from_str::<Self>(val) {
            Ok(t) => Some(Box::new(t)),
            Err(_) => None,
        }
    }
}
