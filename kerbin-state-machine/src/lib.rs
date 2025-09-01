use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use crate::{
    storage::StateStorage,
    system::{System, into_system::IntoSystem},
};

pub mod storage;
pub mod system;

#[derive(Default)]
pub struct State {
    storage: StateStorage,
    hooks: HashMap<TypeId, Vec<Box<dyn System>>>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state<T: Any + Send + Sync + 'static>(&mut self, state: T) -> &mut Self {
        self.storage
            .states
            .insert(state.type_id(), Box::new(Arc::new(RwLock::new(state))));
        self
    }

    pub fn on_hook<H: 'static>(&mut self) -> HookBuilder<'_> {
        HookBuilder {
            type_id: TypeId::of::<H>(),
            state: self,
        }
    }

    pub async fn call_hook<H: 'static>(&self) {
        let Some(hooks) = self.hooks.get(&TypeId::of::<H>()) else {
            return;
        };

        // Get what systems can be run concurrently
        let indices = group_concurrent_system_indices(hooks);

        for group in indices {
            let mut futures = vec![];
            for indice in group {
                futures.push(hooks[indice].call(&self.storage));
            }

            futures::future::join_all(futures).await;
        }
    }
}

pub struct HookBuilder<'a> {
    type_id: TypeId,
    state: &'a mut State,
}

impl<'a> HookBuilder<'a> {
    pub fn system<I, D, S: System + 'static>(
        &mut self,
        sys: impl IntoSystem<I, D, System = S>,
    ) -> &mut Self {
        let system = sys.into_system();
        guarentee_params(&system);
        self.state
            .hooks
            .entry(self.type_id)
            .or_default()
            .push(Box::new(system));
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

    grouped_index_sets
}

/// Verifies that no params are overlapping
fn guarentee_params<S: System>(system: &S) {
    let params = system.params();
    let param_count = params.len();
    let mut param_set = HashSet::new();
    for param in &params {
        if param.reserved && param_count > 1 {
            panic!(
                "System has too many arguments to have a reserved argument, please only take one reserved arg in any given function"
            )
        }
        if !param_set.insert(param.type_id) {
            panic!(
                "The same type was requested by the system more than once, please ensure you're only requesting the type once."
            )
        };
    }
}
