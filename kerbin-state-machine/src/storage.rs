use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct StateStorage {
    pub states: HashMap<String, Box<dyn StateName>>,
}

pub trait StateName: Any + Send + Sync + 'static {
    /// Returns the concatenated (crate_name::module::Type) type_name
    /// This is consistent across the ffi boundary
    fn name(&self) -> String;
}

impl<S: StateName + StaticState> StateName for Arc<RwLock<S>> {
    fn name(&self) -> String {
        S::static_name()
    }
}

impl<S: StateName + StaticState> StaticState for Arc<RwLock<S>> {
    fn static_name() -> String {
        S::static_name()
    }
}

pub trait StaticState {
    fn static_name() -> String;
}

impl dyn StateName {
    pub fn downcast<S: StateName + StaticState>(&self) -> Option<&Arc<RwLock<S>>> {
        if S::static_name() == self.name() {
            Some(unsafe { &*(self as *const dyn StateName as *const Arc<RwLock<S>>) })
        } else {
            None
        }
    }
}
