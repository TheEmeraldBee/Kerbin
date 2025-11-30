use std::{collections::HashMap, path::Path};

use kerbin_core::*;
use lsp_types::Uri;
use serde::Deserialize;
use tokio::process::ChildStdin;

use crate::{LspClient, UriExt};

#[derive(Clone, Deserialize)]
pub struct LangInfo {
    pub command: String,
    pub args: Vec<String>,

    /// A list of paths to look for when finding the root file
    /// When empty, PWD is used as the root of the workspace
    pub roots: Vec<String>,
}

impl LangInfo {
    pub fn new(command: impl ToString) -> Self {
        Self {
            command: command.to_string(),
            args: vec![],
            roots: vec![],
        }
    }

    /// Add an argument to the command
    pub fn with_arg(mut self, arg: impl ToString) -> Self {
        self.args.push(arg.to_string());
        self
    }

    /// Extend the argument list for the command
    pub fn with_args(mut self, args: impl IntoIterator<Item = impl ToString>) -> Self {
        self.args.extend(args.into_iter().map(|x| x.to_string()));
        self
    }

    /// Add a valid root location to the language
    pub fn with_root(mut self, root: impl ToString) -> Self {
        self.roots.push(root.to_string());
        self
    }
}

#[derive(Default, State)]
pub struct LspManager {
    /// Running clients that map a language ID to the client
    pub client_map: HashMap<String, LspClient<ChildStdin>>,

    /// A map for language IDs to Language Information
    pub lang_info_map: HashMap<String, LangInfo>,

    /// A map for language extensions to their respective Language IDs
    pub ext_map: HashMap<String, String>,
}

impl LspManager {
    pub fn register_language(
        &mut self,
        name: impl ToString,
        exts: impl IntoIterator<Item = impl ToString>,
        language_info: LangInfo,
    ) {
        let name = name.to_string();
        for ext in exts.into_iter() {
            self.ext_map.insert(ext.to_string(), name.clone());
        }

        self.lang_info_map.insert(name, language_info);
    }

    /// Retrieves a running client, creating it if non-existant
    ///
    /// Will return None if there is no language description and
    /// the client isn't already running
    pub async fn get_or_create_client(
        &mut self,
        lang: impl ToString,
    ) -> Option<&mut LspClient<ChildStdin>> {
        let lang = lang.to_string();
        if self.client_map.contains_key(&lang) {
            return Some(
                self.client_map
                    .get_mut(&lang)
                    .expect("Client should exist, it was just looked for"),
            );
        }

        let info = self.lang_info_map.get(&lang)?;

        let client = LspClient::spawned(&lang, &info.command, info.args.clone())
            .await
            .unwrap();

        self.client_map.insert(lang.clone(), client);

        Some(
            self.client_map
                .get_mut(&lang)
                .expect("Client should exist, it was just inserted"),
        )
    }
}

/// Helper function to find workspace root based on root files
pub fn find_workspace_root(file_path: &str, lang_info: Option<&LangInfo>) -> Option<Uri> {
    let lang_info = lang_info?;
    let path = Path::new(file_path);
    let mut current = path.parent()?;

    // Search upwards for root markers
    loop {
        for root_marker in &lang_info.roots {
            let marker_path = current.join(root_marker);
            if marker_path.exists() {
                return Uri::file_path(current.to_str().unwrap()).ok();
            }
        }

        current = current.parent()?;
    }
}
