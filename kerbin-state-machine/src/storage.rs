use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct StateStorage {
    pub states: HashMap<TypeId, Box<dyn AnyResourceRwLock>>,
}

pub trait AnyResourceRwLock: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Send + Sync + 'static> AnyResourceRwLock for Arc<RwLock<T>> {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
