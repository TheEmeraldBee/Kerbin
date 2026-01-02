use kerbin_state_machine::{
    State, run_system_groups,
    system::{System, into_system::IntoSystem},
};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    marker::PhantomData,
    sync::{Arc, LazyLock},
};
use tokio::sync::{RwLock, RwLockWriteGuard};

use crate::*;

struct EventEntry {
    active: bool,
    subscribers: Vec<Box<dyn System + Send + Sync>>,
    data: Option<Box<dyn Any + Send + Sync>>,
}

#[derive(Default)]
pub struct TypedBus {
    map: RwLock<HashMap<TypeId, EventEntry>>,
}

impl TypedBus {
    /// Emit an event with data
    pub async fn emit<T: 'static + Send + Sync>(&self, data: T) {
        let type_id = TypeId::of::<T>();

        // Mark event as active
        let mut map = self.map.write().await;
        let entry = map.entry(type_id).or_insert(EventEntry {
            active: true,
            subscribers: Vec::new(),
            data: None,
        });

        entry.active = true;
        entry.data = Some(Box::new(Arc::new(data)));
    }

    /// Emit an event without data (for marker events)
    pub async fn emit_marker<T: 'static>(&self) {
        let mut map = self.map.write().await;
        map.entry(TypeId::of::<T>())
            .or_insert(EventEntry {
                active: true,
                subscribers: Vec::new(),
                data: None,
            })
            .active = true;
    }

    /// Add a subscriber system for a specific event type
    pub async fn subscribe<T: 'static>(&self) -> SubscriberBuilder<'_> {
        let map = self.map.write().await;

        SubscriberBuilder {
            bus_map: map,
            entry_type: TypeId::of::<T>(),
        }
    }

    /// Handle events that were emitted
    pub async fn resolve(&self, state: &mut State) {
        let mut map = self.map.write().await;
        for (_, entry) in map.iter_mut() {
            if !entry.active {
                continue;
            }
            entry.active = false;

            state
                .lock_state::<EventStorage>()
                .await
                .set(entry.data.take());

            // Run the systems concurrently
            run_system_groups(&entry.subscribers, &state.storage).await;
        }
    }
}

pub struct SubscriberBuilder<'a> {
    bus_map: RwLockWriteGuard<'a, HashMap<TypeId, EventEntry>>,
    entry_type: TypeId,
}

impl<'a> SubscriberBuilder<'a> {
    pub fn system<I, D, S: System + Send + Sync + 'static>(
        &mut self,
        system: impl IntoSystem<I, D, System = S>,
    ) {
        self.bus_map
            .entry(self.entry_type)
            .or_insert(EventEntry {
                active: false,
                subscribers: vec![],
                data: None,
            })
            .subscribers
            .push(Box::new(system.into_system()))
    }
}

#[derive(State, Default)]
pub struct EventStorage {
    inner: Option<Box<dyn Any + Send + Sync + 'static>>,
}

impl EventStorage {
    pub fn set(&mut self, value: Option<Box<dyn Any + Send + Sync + 'static>>) {
        self.inner = value;
    }

    pub fn get<T: 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .as_ref()
            .and_then(|x| x.downcast_ref::<Arc<T>>().cloned())
    }
}

pub struct EventData<T: Send + Sync + 'static> {
    value: Arc<RwLock<EventStorage>>,
    phantom_t: PhantomData<T>,
}

#[async_trait::async_trait]
impl<T: Send + Sync + 'static> SystemParam for EventData<T> {
    type Item<'new> = EventData<T>;
    fn retrieve(resources: &StateStorage) -> Self::Item<'_> {
        let storage = resources
            .states
            .get(&EventStorage::static_name())
            .unwrap()
            .downcast::<EventStorage>()
            .unwrap()
            .clone();

        EventData {
            value: storage,
            phantom_t: PhantomData::<T>,
        }
    }

    type Inner<'a> = Option<Arc<T>>;
    async fn get(&self) -> Self::Inner<'_> {
        let read_inner = self.value.clone().read_owned().await;

        read_inner.get::<T>()
    }

    fn desc() -> SystemParamDesc {
        SystemParamDesc::new::<EventStorage>(false)
    }
}

pub static EVENT_BUS: LazyLock<Arc<TypedBus>> = LazyLock::new(|| Arc::new(TypedBus::default()));
