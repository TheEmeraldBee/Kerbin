use futures::future::BoxFuture;

use crate::{storage::StateStorage, system::param::SystemParamDesc};

pub mod function_system;
pub mod into_system;

pub mod param;

pub trait System {
    fn call<'a>(&'a self, storage: &StateStorage) -> BoxFuture<'a, ()>;

    fn params(&self) -> Vec<SystemParamDesc>;
}
