use std::marker::PhantomData;
use std::sync::RwLockWriteGuard;
use std::sync::{Arc, RwLock};

use ascii_forge::math::Vec2;
use ascii_forge::window::Buffer;
use kerbin_state_machine::storage::*;
use kerbin_state_machine::system::param::SystemParamDesc;
use kerbin_state_machine::system::param::res::Res;

use crate::Chunks;

use super::SystemParam;

pub struct Chunk<T: StateName + StaticState + 'static> {
    value: Option<Arc<RwLock<Buffer>>>,
    phantom_t: PhantomData<T>,
}

impl<T: StateName + StaticState> StaticState for Chunk<T> {
    fn static_name() -> String {
        format!("chunk::{}", T::static_name())
    }
}

impl<T: StateName + StaticState> StateName for Chunk<T> {
    fn name(&self) -> String {
        Self::static_name()
    }
}

impl<T: StateName + StaticState> SystemParam for Chunk<T> {
    type Item<'new> = Chunk<T>;
    fn retrieve(resources: &StateStorage) -> Self::Item<'_> {
        let chunks = Res::<Chunks>::retrieve(resources);
        let chunks = chunks.get();

        let buf = chunks.get_chunk::<T>();

        Chunk {
            value: buf,
            phantom_t: PhantomData::<T>::default(),
        }
    }

    type Inner<'a> = Option<RwLockWriteGuard<'a, Buffer>>;
    fn get(&self) -> Self::Inner<'_> {
        match self.value.as_ref() {
            Some(t) => Some(t.write().unwrap()),
            None => None,
        }
    }

    fn desc() -> SystemParamDesc {
        SystemParamDesc::new::<Chunks>(false)
    }
}
