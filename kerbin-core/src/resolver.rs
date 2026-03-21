use std::{
    collections::HashMap,
    io::BufRead,
    sync::{Arc, LazyLock},
};

use kerbin_input::{CommandExecutor, ParseError, Resolver, Token};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Default)]
pub struct ResolverEngine {
    custom_fn: Option<Arc<CommandExecutor>>,

    map: HashMap<String, Token>,
}

impl ResolverEngine {
    pub fn as_resolver(&self) -> Resolver<'_> {
        let default_fn = move |cmd: &str, args: &[String]| {
            let output = std::process::Command::new(cmd)
                .args(args)
                .output()
                .map_err(|e| ParseError::Custom(e.to_string()))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let msg = if stderr.is_empty() {
                    format!("exited with {}", output.status)
                } else {
                    format!("exited with {}: {}", output.status, stderr)
                };
                return Err(ParseError::Custom(msg));
            }

            Ok(output
                .stdout
                .lines()
                .map(|l| l.unwrap())
                .collect::<Vec<_>>())
        };

        Resolver::new(
            &self.map,
            self.custom_fn.clone().unwrap_or(Arc::new(default_fn)),
        )
    }

    pub fn templates(&self) -> &HashMap<String, Token> {
        &self.map
    }

    pub fn set_cmd_fn(&mut self, solver: Option<Arc<CommandExecutor>>) {
        self.custom_fn = solver;
    }

    pub fn extend_map(&mut self, map: HashMap<String, Token>) {
        self.map.extend(map);
    }

    pub fn set_template(&mut self, name: impl ToString, value: impl Into<Token>) {
        self.map.insert(name.to_string(), value.into());
    }

    pub fn remove_template(&mut self, template: impl AsRef<str>) {
        self.map.remove(template.as_ref());
    }

    pub fn get_template(&self, template: impl AsRef<str>) -> Option<&Token> {
        self.map.get(template.as_ref())
    }

    pub fn has_template(&self, template: impl AsRef<str>) -> bool {
        self.map.contains_key(template.as_ref())
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
