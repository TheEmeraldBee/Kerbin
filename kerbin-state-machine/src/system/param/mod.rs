use std::any::{Any, TypeId};

use crate::storage::StateStorage;

pub mod res;
pub mod res_mut;

pub trait SystemParam {
    type Item<'new>: Send + Sync;
    fn retrieve(resources: &StateStorage) -> Self::Item<'_>;

    type Inner<'a>
    where
        Self: 'a;
    fn get(&self) -> Self::Inner<'_>;

    fn desc() -> SystemParamDesc;
}

#[derive(Copy, Clone, Debug)]
pub struct SystemParamDesc {
    pub type_id: TypeId,
    pub reserved: bool,
    pub write: bool,
}

impl SystemParamDesc {
    pub fn new<T: Any>(write: bool) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            write,
            reserved: false,
        }
    }
    pub fn new_reserved<T: Any>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            write: true,
            reserved: true,
        }
    }
}
