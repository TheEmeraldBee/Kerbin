use std::marker::PhantomData;
use std::sync::Arc;

use kerbin_state_machine::storage::*;
use kerbin_state_machine::system::param::SystemParamDesc;
use tokio::sync::{OwnedRwLockWriteGuard, RwLock};

use crate::{Chunks, InnerChunk};

use crate::SystemParam;

/// A SystemRes that stores a chunk used for making the rendering of a chunk faster and easier
pub struct Chunk<T: StateName + StaticState + 'static> {
    value: Arc<RwLock<Chunks>>,
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

#[async_trait::async_trait]
impl<T: StateName + StaticState> SystemParam for Chunk<T> {
    type Item<'new> = Chunk<T>;
    fn retrieve(resources: &StateStorage) -> Self::Item<'_> {
        let chunks = resources
            .states
            .get(&Chunks::static_name())
            .unwrap()
            .downcast::<Chunks>()
            .unwrap()
            .clone();

        Chunk {
            value: chunks,
            phantom_t: PhantomData::<T>,
        }
    }

    type Inner<'a> = Option<OwnedRwLockWriteGuard<InnerChunk>>;
    async fn get(&self) -> Self::Inner<'_> {
        let read_inner = self.value.clone().read_owned().await;
        if let Some(x) = read_inner.get_chunk::<T>() {
            Some(x.clone().write_owned().await)
        } else {
            None
        }
    }

    fn desc() -> SystemParamDesc {
        SystemParamDesc::new::<Chunks>(false)
    }
}
