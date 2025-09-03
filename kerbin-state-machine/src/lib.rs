use std::{
    any::TypeId,
    collections::HashSet,
    sync::{Arc, RwLock, RwLockWriteGuard},
};

use crate::{
    storage::{StateName, StateStorage, StaticState},
    system::{System, into_system::IntoSystem},
};

pub mod storage;
pub mod system;

pub trait Hook {
    fn info(&self) -> HookInfo;
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookPathComponent {
    Wildcard,
    Path(String),
    OneOf(Vec<String>),
}

impl HookPathComponent {
    pub fn parse(input: &str) -> Vec<HookPathComponent> {
        let mut res = vec![];
        let parts = input.split("::");

        for part in parts {
            res.push(if part == "*" {
                HookPathComponent::Wildcard
            } else if part.contains("|") {
                let options: Vec<String> = part.split("|").map(|s| s.trim().to_string()).collect();
                HookPathComponent::OneOf(options)
            } else {
                HookPathComponent::Path(part.to_string())
            });
        }

        res
    }

    pub fn default_rank(input: &[HookPathComponent]) -> i8 {
        let mut rank = 0;

        for component in input {
            match component {
                HookPathComponent::Wildcard => rank -= 2,
                HookPathComponent::OneOf(_) => rank -= 1,
                HookPathComponent::Path(_) => {}
            }
        }

        rank
    }
}

pub struct HookInfo {
    pub path: Vec<HookPathComponent>,
    pub rank: i8,
}

impl HookInfo {
    pub fn new(path: &str) -> Self {
        let path = HookPathComponent::parse(path);
        Self {
            rank: HookPathComponent::default_rank(&path),
            path,
        }
    }

    pub fn matches(&self, path: &[HookPathComponent]) -> Option<i8> {
        let mut matches = true;

        for (path, component) in path.iter().zip(self.path.iter()) {
            matches = match (component, path) {
                (HookPathComponent::Wildcard, _) => true,
                (HookPathComponent::Path(s), HookPathComponent::Path(p)) => p == s,
                (HookPathComponent::OneOf(options), HookPathComponent::Path(p)) => {
                    options.contains(p)
                }
                (_, _) => true,
            };

            if !matches {
                break;
            }
        }

        if matches { Some(self.rank) } else { None }
    }
}

#[macro_export]
macro_rules! get {
    (@inner $name:ident $(, $($t:tt)+)?) => {
        let $name = $name.get();
        get!(@inner $($($t)+)?)
    };
    (@inner mut $name:ident $(, $($t:tt)+)?) => {
        let mut $name = $name.get();
        get!(@inner $($($t)*)?)
    };
    (@inner $($t:tt)+) => {
        compile_error!("Expected comma-separated list of (mut item) or (item), but got an error while parsing. Make sure you don't have a trailing `,`");
    };
    (@inner) => {};
    ($($t:tt)*) => {
        get!(@inner $($t)*)
    };
}

#[derive(Default)]
pub struct State {
    pub storage: StateStorage,
    hooks: Vec<(HookInfo, Vec<Box<dyn System>>)>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state<T: StateName + StaticState + 'static>(&mut self, state: T) -> &mut Self {
        self.storage
            .states
            .insert(T::static_name(), Box::new(Arc::new(RwLock::new(state))));
        self
    }

    pub fn lock_state<'a, S: StateName + StaticState>(&'a self) -> Option<RwLockWriteGuard<'a, S>> {
        Some(
            self.storage
                .states
                .get(&S::static_name())?
                .downcast::<S>()?
                .write()
                .unwrap(),
        )
    }

    pub fn on_hook<H: Hook>(&mut self, hook: H) -> HookBuilder<'_, H> {
        HookBuilder { hook, state: self }
    }

    pub async fn call<I, D>(&self, sys: impl IntoSystem<I, D>) {
        sys.into_system().call(&self.storage).await
    }

    pub fn hook(&self, hook: impl Hook + 'static) -> HookCallBuilder<'_> {
        HookCallBuilder {
            state: self,
            hooks: vec![Box::new(hook)],
        }
    }
}

pub struct HookCallBuilder<'a> {
    state: &'a State,
    hooks: Vec<Box<dyn Hook>>,
}

impl<'a> HookCallBuilder<'a> {
    pub fn hook(mut self, hook: impl Hook + 'static) -> Self {
        self.hooks.push(Box::new(hook));
        self
    }

    pub async fn call(self) {
        for hook in self.hooks {
            let path = hook.info().path;

            let mut most_valid_hooks = None;
            for (info, hooks) in self.state.hooks.iter() {
                if let Some(new_rank) = info.matches(&path) {
                    let mut old_rank = i8::MIN;

                    if let Some((rank, _)) = most_valid_hooks.as_ref() {
                        old_rank = *rank;
                    }

                    if old_rank < new_rank {
                        most_valid_hooks = Some((new_rank, hooks))
                    }
                }
            }

            let Some((_, hooks)) = most_valid_hooks else {
                continue;
            };

            let indices = group_concurrent_system_indices(hooks);

            for group in indices {
                let mut futures = vec![];
                for indice in group {
                    futures.push(hooks[indice].call(&self.state.storage));
                }

                futures::future::join_all(futures).await;
            }
        }
    }
}

pub struct HookBuilder<'a, H: Hook> {
    hook: H,
    state: &'a mut State,
}

impl<'a, H: Hook> HookBuilder<'a, H> {
    pub fn system<I, D, S: System + 'static>(
        &mut self,
        sys: impl IntoSystem<I, D, System = S>,
    ) -> &mut Self {
        let system = sys.into_system();
        guarentee_params(&system);
        let hook_info = self.hook.info();
        let entry = self
            .state
            .hooks
            .iter_mut()
            .find(|x| x.0.path == hook_info.path);

        if let Some(entry) = entry {
            entry.1.push(Box::new(system))
        } else {
            self.state.hooks.push((hook_info, vec![Box::new(system)]));
        }

        self
    }
}

fn group_concurrent_system_indices(systems: &[Box<dyn System>]) -> Vec<Vec<usize>> {
    let mut system_indices: Vec<usize> = (0..systems.len()).collect();
    let mut grouped_index_sets: Vec<Vec<usize>> = Vec::new();

    // Sort original indices to ensure deterministic grouping (optional but good for testing)
    system_indices.sort();

    while !system_indices.is_empty() {
        let mut current_group_indices: Vec<usize> = Vec::new();
        let mut types_in_group: HashSet<TypeId> = HashSet::new();
        let mut write_types_in_group: HashSet<TypeId> = HashSet::new();
        let mut indices_to_remove_from_system_indices: Vec<usize> = Vec::new();

        'outer: for (i, &system_idx) in system_indices.iter().enumerate() {
            let system_params = systems[system_idx].params();

            let mut conflicts = false;
            let mut is_reserved_system = false;

            // Check if this system has any reserved parameters
            for param in &system_params {
                if param.reserved {
                    is_reserved_system = true;
                    break;
                }
            }

            if is_reserved_system {
                // If a reserved system, it must run alone.
                // If current_group is empty, add it and move on.
                // Otherwise, it cannot join the current group.
                if current_group_indices.is_empty() {
                    current_group_indices.push(system_idx);
                    indices_to_remove_from_system_indices.push(i);
                    // No other systems can be in this group with a reserved system
                    break 'outer;
                } else {
                    // Cannot add a reserved system to an already forming group
                    continue;
                }
            }

            // If the current group already contains a reserved system,
            // no other systems can be added.
            if !current_group_indices.is_empty() {
                for &existing_sys_idx in &current_group_indices {
                    for existing_param in systems[existing_sys_idx].params() {
                        if existing_param.reserved {
                            conflicts = true;
                            break;
                        }
                    }
                    if conflicts {
                        break;
                    }
                }
            }

            if conflicts {
                continue;
            }

            // Check for conflicts with existing items in the current_group
            let mut potential_types_in_group = types_in_group.clone();
            let mut potential_write_types_in_group = write_types_in_group.clone();
            let mut local_conflicts = false;

            for param in &system_params {
                if param.write {
                    // If it's a write, conflict if any item in the group
                    // (read or write) uses the same type_id
                    if types_in_group.contains(&param.type_id) {
                        local_conflicts = true;
                        break;
                    }
                } else {
                    // If it's a read, conflict if any write item in the group
                    // uses the same type_id
                    if write_types_in_group.contains(&param.type_id) {
                        local_conflicts = true;
                        break;
                    }
                }
                potential_types_in_group.insert(param.type_id);
                if param.write {
                    potential_write_types_in_group.insert(param.type_id);
                }
            }

            if !local_conflicts {
                current_group_indices.push(system_idx);
                indices_to_remove_from_system_indices.push(i);
                types_in_group = potential_types_in_group;
                write_types_in_group = potential_write_types_in_group;
            }
        }

        // Add the formed group
        if !current_group_indices.is_empty() {
            grouped_index_sets.push(current_group_indices);
        }

        // Remove the processed items from `system_indices`
        // Sort in reverse to remove from the end first, avoiding index issues
        indices_to_remove_from_system_indices.sort_by(|a, b| b.cmp(a));
        for i in indices_to_remove_from_system_indices {
            system_indices.remove(i);
        }
    }

    tracing::warn!("{:?}", grouped_index_sets);

    grouped_index_sets
}

/// Verifies that no params are overlapping
fn guarentee_params<S: System>(system: &S) {
    let params = system.params();
    let param_count = params.len();
    let mut param_set = HashSet::new();
    for param in &params {
        if param.reserved && param_count > 1 {
            tracing::info!("Hek");

            panic!(
                "System has too many arguments to have a reserved argument, please only take one reserved arg in any given function"
            )
        }
        if !param_set.insert(param.type_id) {
            tracing::info!("Hek");

            panic!(
                "The same type was requested by the system more than once, please ensure you're only requesting the type once."
            )
        };
    }
}
