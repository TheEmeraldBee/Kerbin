use std::{collections::HashMap, path::Path};

use kerbin_core::*;
use lsp_types::Uri;
use serde::Deserialize;
use tokio::process::ChildStdin;

use crate::{LspClient, UriExt};

#[derive(Clone)]
pub enum FormatterKind {
    Lsp,
    External(String, Vec<String>),
}

#[derive(Clone)]
pub struct FormatterConfig {
    pub kind: FormatterKind,
    pub format_on_save: bool,
}

#[derive(Clone, Deserialize)]
pub struct LangInfo {
    pub command: String,
    pub args: Vec<String>,

    /// A list of paths to look for when finding the root file.
    /// When empty, PWD is used as the root of the workspace.
    pub roots: Vec<String>,

    #[serde(skip)]
    pub format: Option<FormatterConfig>,
}

impl LangInfo {
    pub fn new(command: impl ToString) -> Self {
        Self {
            command: command.to_string(),
            args: vec![],
            roots: vec![],
            format: None,
        }
    }

    pub fn with_lsp_format(mut self, on_save: bool) -> Self {
        self.format = Some(FormatterConfig {
            kind: FormatterKind::Lsp,
            format_on_save: on_save,
        });
        self
    }

    pub fn with_external_format(
        mut self,
        cmd: impl ToString,
        args: Vec<String>,
        on_save: bool,
    ) -> Self {
        self.format = Some(FormatterConfig {
            kind: FormatterKind::External(cmd.to_string(), args),
            format_on_save: on_save,
        });
        self
    }

    pub fn with_arg(mut self, arg: impl ToString) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = impl ToString>) -> Self {
        self.args.extend(args.into_iter().map(|x| x.to_string()));
        self
    }

    pub fn with_root(mut self, root: impl ToString) -> Self {
        self.roots.push(root.to_string());
        self
    }

    pub fn with_roots(mut self, roots: impl IntoIterator<Item = impl ToString>) -> Self {
        self.roots.extend(roots.into_iter().map(|x| x.to_string()));
        self
    }
}

#[derive(Default, State)]
pub struct LspManager {
    /// Server name → language server config (command, args, roots, formatter)
    pub server_map: HashMap<String, LangInfo>,

    /// Language name → server name (many languages can share one server)
    pub lang_to_server: HashMap<String, String>,

    /// Running clients keyed by server name
    pub client_map: HashMap<String, LspClient<ChildStdin>>,

    /// Server names whose process failed to spawn; won't retry until lsp-restart
    pub spawn_failed: std::collections::HashSet<String>,
}

impl LspManager {
    pub fn register_server(
        &mut self,
        server_name: impl Into<String>,
        langs: impl IntoIterator<Item = impl Into<String>>,
        info: LangInfo,
    ) {
        let server_name = server_name.into();
        self.server_map.insert(server_name.clone(), info);
        for lang in langs {
            self.lang_to_server.insert(lang.into(), server_name.clone());
        }
    }

    /// Resolve a language name to its server name, if registered.
    pub fn server_for_lang(&self, lang: &str) -> Option<&str> {
        self.lang_to_server.get(lang).map(|s| s.as_str())
    }

    /// Retrieves a running client for the given language, creating it if needed.
    ///
    /// Returns `None` (no error) if the language has no registered server.
    /// Returns `Err` if the server is registered but failed to spawn.
    pub async fn get_or_create_client(
        &mut self,
        lang: &str,
    ) -> Result<Option<&mut LspClient<ChildStdin>>, std::io::Error> {
        let Some(server_name) = self.lang_to_server.get(lang).cloned() else {
            return Ok(None);
        };

        if self.spawn_failed.contains(&server_name) {
            return Ok(None);
        }

        if !self.client_map.contains_key(&server_name) {
            let Some(info) = self.server_map.get(&server_name) else {
                return Ok(None);
            };

            let client =
                match LspClient::spawned(&server_name, &info.command, info.args.clone()).await {
                    Ok(c) => c,
                    Err(e) => {
                        self.spawn_failed.insert(server_name);
                        return Err(e);
                    }
                };

            self.client_map.insert(server_name.clone(), client);
        }

        Ok(self.client_map.get_mut(&server_name))
    }

    /// Returns a human-readable status string for the server handling the given language.
    pub fn lang_status(&self, lang: &str) -> String {
        let Some(server_name) = self.lang_to_server.get(lang) else {
            return "unknown language".to_string();
        };

        if let Some(client) = self.client_map.get(server_name) {
            if client.is_initialized() {
                format!("{server_name}: running (initialized)")
            } else {
                format!("{server_name}: running (not yet initialized)")
            }
        } else if self.spawn_failed.contains(server_name) {
            format!("{server_name}: spawn failed (use lsp-restart to retry)")
        } else {
            format!("{server_name}: registered (not started)")
        }
    }

    /// Removes the running/failed client for the language's server so it can be respawned.
    ///
    /// Returns the server name if anything was removed, `None` otherwise.
    pub fn reset_client(&mut self, lang: &str) -> Option<String> {
        let server_name = self.lang_to_server.get(lang)?.clone();
        let was_running = self.client_map.remove(&server_name).is_some();
        let was_failed = self.spawn_failed.remove(&server_name);
        if was_running || was_failed {
            Some(server_name)
        } else {
            None
        }
    }

    /// Returns all language names served by the given server.
    pub fn langs_for_server(&self, server_name: &str) -> Vec<String> {
        self.lang_to_server
            .iter()
            .filter(|(_, sn)| sn.as_str() == server_name)
            .map(|(lang, _)| lang.clone())
            .collect()
    }

    /// Retrieve the LangInfo for the server that handles the given language.
    pub fn info_for_lang(&self, lang: &str) -> Option<&LangInfo> {
        let server_name = self.lang_to_server.get(lang)?;
        self.server_map.get(server_name)
    }
}

/// Helper function to find workspace root based on root files
pub fn find_workspace_root(file_path: &str, lang_info: Option<&LangInfo>) -> Option<Uri> {
    let lang_info = lang_info?;
    let path = Path::new(file_path);
    let mut current = path.parent()?;

    loop {
        for root_marker in &lang_info.roots {
            let marker_path = current.join(root_marker);
            if marker_path.exists() {
                return current.to_str().and_then(|s| Uri::file_path(s).ok());
            }
        }

        current = current.parent()?;
    }
}
