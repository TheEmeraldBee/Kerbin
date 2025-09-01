use std::any::type_name;

use std::any::TypeId;
use std::sync::{Arc, RwLock, RwLockReadGuard};

use crate::storage::StateStorage;
use crate::system::param::SystemParamDesc;

use super::SystemParam;

pub struct Res<T: Send + Sync + 'static> {
    value: Arc<RwLock<T>>,
}

impl<T: Send + Sync + 'static> SystemParam for Res<T> {
    type Item<'new> = Res<T>;
    fn retrieve(resources: &StateStorage) -> Self::Item<'_> {
        let arc_rwlock_any = resources.states.get(&TypeId::of::<T>()).unwrap_or_else(|| {
            panic!(
                "Resource: `{}` with id `{:?}` Not Found",
                type_name::<T>(),
                TypeId::of::<T>()
            )
        });

        let arc_rwlock_t = arc_rwlock_any
            .as_any()
            .downcast_ref::<Arc<RwLock<T>>>()
            .unwrap_or_else(|| {
                panic!(
                    "Failed to downcast stored RwLock to RwLock<{}>",
                    type_name::<T>()
                )
            })
            .clone();

        Res {
            value: arc_rwlock_t,
        }
    }

    type Inner<'a> = RwLockReadGuard<'a, T>;
    fn get(&self) -> Self::Inner<'_> {
        self.value.read().unwrap()
    }

    fn desc() -> SystemParamDesc {
        SystemParamDesc::new::<T>(false)
    }
}
