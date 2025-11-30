use std::{
    collections::HashMap,
    io::BufRead,
    sync::{Arc, LazyLock},
};

use kerbin_input::{CommandExecutor, ParseError, Resolver};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Default)]
pub struct ResolverEngine {
    custom_fn: Option<Arc<CommandExecutor>>,

    map: HashMap<String, Vec<String>>,
}

impl ResolverEngine {
    pub fn as_resolver(&self) -> Resolver<'_> {
        let default_fn = move |cmd: &str, args: &[String]| {
            std::process::Command::new(cmd)
                .args(args)
                .output()
                .map_err(|e| ParseError::Custom(e.to_string()))
                .map(|o| o.stdout.lines().map(|l| l.unwrap()).collect::<Vec<_>>())
        };

        Resolver::new(
            &self.map,
            self.custom_fn.clone().unwrap_or(Arc::new(default_fn)),
        )
    }

    pub fn set_cmd_fn(&mut self, solver: Option<Arc<CommandExecutor>>) {
        self.custom_fn = solver;
    }

    pub fn extend_map(&mut self, map: HashMap<String, Vec<String>>) {
        self.map.extend(map);
    }

    pub fn set_template(
        &mut self,
        template: impl ToString,
        value: impl IntoIterator<Item = impl ToString>,
    ) {
        self.map.insert(
            template.to_string(),
            value.into_iter().map(|x| x.to_string()).collect(),
        );
    }

    pub fn trash_template(&mut self, template: impl AsRef<str>) {
        self.map.remove(template.as_ref());
    }
}

pub async fn resolver_engine() -> OwnedRwLockReadGuard<ResolverEngine> {
    RESOLVER_ENGINE.clone().read_owned().await
}

pub async fn resolver_engine_mut() -> OwnedRwLockWriteGuard<ResolverEngine> {
    RESOLVER_ENGINE.clone().write_owned().await
}

static RESOLVER_ENGINE: LazyLock<Arc<RwLock<ResolverEngine>>> =
    LazyLock::new(|| Arc::new(RwLock::new(ResolverEngine::default())));
