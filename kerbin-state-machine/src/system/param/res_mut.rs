use std::any::type_name;

use std::any::TypeId;
use std::sync::RwLockWriteGuard;
use std::sync::{Arc, RwLock};

use crate::storage::StateName;
use crate::storage::StateStorage;
use crate::storage::StaticState;
use crate::system::param::SystemParamDesc;

use super::SystemParam;

pub struct ResMut<T: StateName + StaticState + 'static> {
    value: Arc<RwLock<T>>,
}

impl<T: StateName + StaticState> SystemParam for ResMut<T> {
    type Item<'new> = ResMut<T>;
    fn retrieve(resources: &StateStorage) -> Self::Item<'_> {
        let arc_rwlock_any = resources.states.get(&T::static_name()).unwrap_or_else(|| {
            panic!(
                "Resource: `{}` with id `{:?}` Not Found",
                type_name::<T>(),
                TypeId::of::<T>()
            )
        });

        let arc_rwlock_t = arc_rwlock_any
            .downcast::<T>()
            .unwrap_or_else(|| {
                panic!(
                    "Failed to downcast stored RwLock to RwLock<{}>",
                    type_name::<T>()
                )
            })
            .clone();

        ResMut {
            value: arc_rwlock_t,
        }
    }

    type Inner<'a> = RwLockWriteGuard<'a, T>;
    fn get(&self) -> Self::Inner<'_> {
        self.value.write().unwrap()
    }

    fn desc() -> SystemParamDesc {
        SystemParamDesc::new::<T>(false)
    }
}
