use std::{collections::HashSet, sync::Arc};

use tokio::sync::{RwLock, RwLockWriteGuard};

use crate::system::{System, into_system::IntoSystem};

pub mod storage;
pub mod system;

pub use storage::*;
pub use system::param::{SystemParam, SystemParamDesc, res::Res, res_mut::ResMut};

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
        Self::parse_custom_split(input, "::")
    }

    pub fn parse_custom_split(input: &str, split: &str) -> Vec<HookPathComponent> {
        let mut res = vec![];
        let parts = input.split(split);

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

    pub fn new_custom_split(path: &str, split: &str) -> Self {
        let path = HookPathComponent::parse_custom_split(path, split);
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
    hooks: Vec<(HookInfo, Vec<Box<dyn System + Send + Sync>>)>,
}

impl State {
    pub fn new() -> Self {
        Self {
            storage: StateStorage::default(),
            ..Default::default()
        }
    }

    pub fn state<T: StateName + StaticState + 'static>(&mut self, state: T) -> &mut Self {
        self.storage
            .states
            .insert(T::static_name(), Box::new(Arc::new(RwLock::new(state))));
        self
    }

    pub async fn lock_state<'a, S: StateName + StaticState>(
        &'a self,
    ) -> Option<RwLockWriteGuard<'a, S>> {
        Some(
            self.storage
                .states
                .get(&S::static_name())?
                .downcast::<S>()?
                .write()
                .await,
        )
    }

    pub fn set_hook<H: Hook, I, D, S: System + Send + Sync + 'static>(
        &mut self,
        hook: H,
        sys: impl IntoSystem<I, D, System = S>,
    ) -> &mut Self {
        let system = sys.into_system();
        guarentee_params(&system);
        let hook_info = hook.info();
        let entry = self.hooks.iter_mut().find(|x| x.0.path == hook_info.path);

        if let Some(entry) = entry {
            entry.1.clear();
            entry.1.push(Box::new(system));
        } else {
            self.hooks.push((hook_info, vec![Box::new(system)]));
        }

        self
    }

    pub fn on_hook<H: Hook>(&mut self, hook: H) -> HookBuilder<'_, H> {
        HookBuilder { hook, state: self }
    }

    pub async fn call<I, D>(&self, sys: impl IntoSystem<I, D>) {
        let system = sys.into_system();

        system.call(&self.storage).await;
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
                let (_, res) = async_scoped::TokioScope::scope_and_block(|s| {
                    for indice in group {
                        let system_future = hooks[indice].call(&self.state.storage);

                        s.spawn(system_future);
                    }
                });

                for res in res {
                    if let Err(e) = res {
                        panic!("{e}");
                    }
                }
            }
        }
    }
}

pub struct HookBuilder<'a, H: Hook> {
    hook: H,
    state: &'a mut State,
}

impl<'a, H: Hook> HookBuilder<'a, H> {
    pub fn system<I, D, S: System + Send + Sync + 'static>(
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

fn group_concurrent_system_indices(systems: &[Box<dyn System + Send + Sync>]) -> Vec<Vec<usize>> {
    let mut remaining_indices: Vec<usize> = (0..systems.len()).collect();
    let mut groups: Vec<Vec<usize>> = Vec::new();

    while !remaining_indices.is_empty() {
        let mut current_group = Vec::new();
        let mut used_types = HashSet::new();
        let mut write_types = HashSet::new();
        let mut indices_to_remove = Vec::new();

        for (pos, &system_idx) in remaining_indices.iter().enumerate() {
            let system_params = systems[system_idx].params();

            let has_reserved = system_params.iter().any(|p| p.reserved);

            if has_reserved {
                if current_group.is_empty() {
                    current_group.push(system_idx);
                    indices_to_remove.push(pos);
                    break;
                } else {
                    continue;
                }
            }

            if !current_group.is_empty() {
                let group_has_reserved = current_group
                    .iter()
                    .any(|&idx| systems[idx].params().iter().any(|p| p.reserved));
                if group_has_reserved {
                    continue;
                }
            }

            let mut can_add = true;
            let mut conflicting_types = Vec::new();

            for param in &system_params {
                if param.write && used_types.contains(&param.type_name) {
                    can_add = false;
                    if write_types.contains(&param.type_name) {
                        conflicting_types.push((&param.type_name, "write", "existing write"));
                    } else {
                        conflicting_types.push((&param.type_name, "write", "existing read"));
                    }
                    break;
                } else if write_types.contains(&param.type_name) {
                    can_add = false;
                    conflicting_types.push((&param.type_name, "read", "existing write"));
                    break;
                }
            }

            if can_add {
                current_group.push(system_idx);
                indices_to_remove.push(pos);

                for param in &system_params {
                    used_types.insert(param.type_name.clone());
                    if param.write {
                        write_types.insert(param.type_name.clone());
                    }
                }
            }
        }

        if !current_group.is_empty() {
            groups.push(current_group);
        }

        indices_to_remove.sort_by(|a, b| b.cmp(a));
        for pos in indices_to_remove {
            remaining_indices.remove(pos);
        }
    }

    groups
}

fn guarentee_params<S: System>(system: &S) {
    let params = system.params();
    let param_count = params.len();
    let mut param_types = HashSet::new();
    let mut write_types = HashSet::new();

    for param in &params {
        if param.reserved && param_count > 1 {
            panic!(
                "System has too many arguments to have a reserved argument, please only take one reserved arg in any given function"
            )
        }

        if param.write {
            if param_types.contains(param.type_name.as_str()) {
                panic!(
                    "The same type was requested by the system more than once, please ensure you're only requesting the type once."
                );
            }
            write_types.insert(param.type_name.as_str());
        } else if write_types.contains(param.type_name.as_str()) {
            panic!(
                "The same type was requested by the system more than once, please ensure you're only requesting the type once."
            );
        }

        param_types.insert(param.type_name.as_str());
    }
}

#[cfg(test)]
mod tests {
    use crate::system::param::SystemParamDesc;

    use super::*;
    use std::pin::Pin;

    #[derive(Debug, Clone)]
    pub struct MockParam {
        pub type_name: String,
        pub write: bool,
        pub reserved: bool,
    }

    pub struct MockSystem {
        params: Vec<MockParam>,
    }

    impl MockSystem {
        pub fn new(params: Vec<(&str, bool, bool)>) -> Self {
            Self {
                params: params
                    .into_iter()
                    .map(|(name, write, reserved)| MockParam {
                        type_name: name.to_string(),
                        write,
                        reserved,
                    })
                    .collect(),
            }
        }
    }

    impl System for MockSystem {
        fn params(&self) -> Vec<SystemParamDesc> {
            let mut res = vec![];
            for item in &self.params {
                res.push(SystemParamDesc {
                    type_name: item.type_name.clone(),
                    write: item.write,
                    reserved: item.reserved,
                })
            }

            res
        }

        fn call<'a>(
            &'a self,
            _storage: &StateStorage,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {})
        }
    }

    fn create_systems(
        system_configs: Vec<Vec<(&str, bool, bool)>>,
    ) -> Vec<Box<dyn System + Send + Sync + 'static>> {
        system_configs
            .into_iter()
            .map(|config| {
                Box::new(MockSystem::new(config)) as Box<dyn System + Send + Sync + 'static>
            })
            .collect()
    }

    #[test]
    fn test_no_conflicts() {
        let systems = create_systems(vec![
            vec![("TypeA", false, false)],
            vec![("TypeB", false, false)],
            vec![("TypeC", true, false)],
        ]);

        let groups = group_concurrent_system_indices(systems.as_slice());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![0, 1, 2]);
    }

    #[test]
    fn test_multiple_reads_same_type() {
        let systems = create_systems(vec![
            vec![("TypeA", false, false)],
            vec![("TypeA", false, false)],
            vec![("TypeA", false, false)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![0, 1, 2]);
    }

    #[test]
    fn test_write_write_conflict() {
        let systems = create_systems(vec![
            vec![("TypeA", true, false)],
            vec![("TypeA", true, false)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
    }

    #[test]
    fn test_read_write_conflict() {
        let systems = create_systems(vec![
            vec![("TypeA", false, false)],
            vec![("TypeA", true, false)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
    }

    #[test]
    fn test_write_read_conflict() {
        let systems = create_systems(vec![
            vec![("TypeA", true, false)],
            vec![("TypeA", false, false)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
    }

    #[test]
    fn test_reserved_system_alone() {
        let systems = create_systems(vec![
            vec![("TypeA", false, true)],
            vec![("TypeB", false, false)],
            vec![("TypeC", false, false)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1, 2]);
    }

    #[test]
    fn test_multiple_reserved_systems() {
        let systems = create_systems(vec![
            vec![("TypeA", false, true)],
            vec![("TypeB", true, true)],
            vec![("TypeC", false, false)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0]);
        assert_eq!(groups[1], vec![1]);
        assert_eq!(groups[2], vec![2]);
    }

    #[test]
    fn test_complex_scenario() {
        let systems = create_systems(vec![
            vec![("TypeA", false, false)],
            vec![("TypeB", false, false)],
            vec![("TypeA", true, false)],
            vec![("TypeC", false, false)],
            vec![("TypeB", false, false)],
            vec![("TypeD", false, true)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 3);
        assert!(groups[0].contains(&0));
        assert!(groups[0].contains(&1));
        assert!(groups[0].contains(&3));
        assert!(groups[0].contains(&4));

        let system_2_group = groups.iter().find(|g| g.contains(&2)).unwrap();
        assert_eq!(system_2_group.len(), 1);

        let system_5_group = groups.iter().find(|g| g.contains(&5)).unwrap();
        assert_eq!(system_5_group.len(), 1);
    }

    #[test]
    fn test_multi_param_system_conflicts() {
        let systems = create_systems(vec![
            vec![("TypeA", false, false), ("TypeB", true, false)],
            vec![("TypeC", false, false), ("TypeA", false, false)],
            vec![("TypeB", false, false)],
        ]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 2);

        let first_group = &groups[0];
        let second_group = &groups[1];

        assert!(first_group.contains(&0) && first_group.contains(&1));
        assert_eq!(second_group, &vec![2]);
    }

    #[test]
    fn test_reserved_with_multiple_params_fails() {
        let systems = create_systems(vec![vec![("TypeA", false, true), ("TypeB", false, false)]]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![0]);
    }

    #[test]
    fn test_empty_systems() {
        let systems: Vec<Box<dyn System + Send + Sync>> = vec![];
        let groups = group_concurrent_system_indices(&systems);
        assert_eq!(groups.len(), 0);
    }

    #[test]
    fn test_single_system() {
        let systems = create_systems(vec![vec![("TypeA", false, false)]]);

        let groups = group_concurrent_system_indices(&systems);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![0]);
    }

    #[test]
    fn test_guarantee_params_no_conflicts() {
        let system = MockSystem::new(vec![
            ("TypeA", false, false),
            ("TypeB", true, false),
            ("TypeC", false, false),
        ]);

        guarentee_params(&system);
    }

    #[test]
    fn test_guarantee_params_duplicate_read_types() {
        let system = MockSystem::new(vec![
            ("TypeA", false, false),
            ("TypeB", true, false),
            ("TypeA", false, false),
        ]);

        guarentee_params(&system);
    }

    #[test]
    #[should_panic(expected = "The same type was requested by the system more than once")]
    fn test_guarantee_params_duplicate_types_different_access() {
        let system = MockSystem::new(vec![("TypeA", false, false), ("TypeA", true, false)]);

        guarentee_params(&system);
    }

    #[test]
    #[should_panic(expected = "System has too many arguments to have a reserved argument")]
    fn test_guarantee_params_reserved_with_multiple_params() {
        let system = MockSystem::new(vec![("TypeA", false, true), ("TypeB", false, false)]);

        guarentee_params(&system);
    }

    #[test]
    #[should_panic(expected = "System has too many arguments to have a reserved argument")]
    fn test_guarantee_params_multiple_reserved_params() {
        let system = MockSystem::new(vec![("TypeA", false, true), ("TypeB", true, true)]);

        guarentee_params(&system);
    }

    #[test]
    fn test_guarantee_params_single_reserved_param() {
        let system = MockSystem::new(vec![("TypeA", false, true)]);

        guarentee_params(&system);
    }

    #[test]
    fn test_guarantee_params_empty_params() {
        let system = MockSystem::new(vec![]);

        guarentee_params(&system);
    }

    #[test]
    fn test_guarantee_params_single_normal_param() {
        let system = MockSystem::new(vec![("TypeA", false, false)]);

        guarentee_params(&system);
    }

    #[test]
    #[should_panic(expected = "The same type was requested by the system more than once")]
    fn test_guarantee_params_many_duplicates() {
        let system = MockSystem::new(vec![
            ("TypeA", false, false),
            ("TypeB", true, false),
            ("TypeC", false, false),
            ("TypeA", true, false),
            ("TypeB", false, false),
        ]);

        guarentee_params(&system);
    }
}
