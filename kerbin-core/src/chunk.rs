use std::marker::PhantomData;
use std::sync::RwLockWriteGuard;
use std::sync::{Arc, RwLock};

use kerbin_state_machine::storage::*;
use kerbin_state_machine::system::param::SystemParamDesc;
use kerbin_state_machine::system::param::res::Res;

use crate::{Chunks, InnerChunk};

use crate::SystemParam;

/// A SystemRes that stores a chunk
/// used for making the rendering of a chunk faster and easier
pub struct Chunk<T: StateName + StaticState + 'static> {
    value: Option<Arc<RwLock<InnerChunk>>>,
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
            phantom_t: PhantomData::<T>,
        }
    }

    type Inner<'a> = Option<RwLockWriteGuard<'a, InnerChunk>>;
    fn get(&self) -> Self::Inner<'_> {
        self.value.as_ref().map(|x| x.write().unwrap())
    }

    fn desc() -> SystemParamDesc {
        SystemParamDesc::new::<Chunks>(false)
    }
}
