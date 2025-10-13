use futures::future::BoxFuture;

use crate::{storage::StateStorage, system::param::SystemParamDesc};

pub mod function_system;
pub mod into_system;

pub mod param;

pub trait System {
    fn call<'a>(
        &'a self,
        handle: tokio::runtime::Handle,
        storage: &'a StateStorage,
    ) -> BoxFuture<'a, ()>;

    fn params(&self) -> Vec<SystemParamDesc>;
}
