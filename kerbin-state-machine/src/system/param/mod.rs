use crate::storage::{StateName, StateStorage, StaticState};

pub mod res;
pub mod res_mut;

#[async_trait::async_trait]
pub trait SystemParam {
    type Item<'new>: Send + Sync;
    fn retrieve(resources: &StateStorage) -> Self::Item<'_>;

    type Inner<'a>
    where
        Self: 'a;
    async fn get(&self) -> Self::Inner<'_>;

    fn desc() -> SystemParamDesc;
}

#[derive(Clone, Debug)]
pub struct SystemParamDesc {
    pub type_name: String,
    pub reserved: bool,
    pub write: bool,
}

impl SystemParamDesc {
    pub fn new<T: StateName + StaticState>(write: bool) -> Self {
        Self {
            type_name: T::static_name(),
            write,
            reserved: false,
        }
    }
    pub fn new_reserved<T: StateName + StaticState>() -> Self {
        Self {
            type_name: T::static_name(),
            write: true,
            reserved: true,
        }
    }
}
