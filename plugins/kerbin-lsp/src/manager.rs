use std::collections::HashMap;

use kerbin_core::{kerbin_macros::State, *};
use tokio::process::ChildStdin;

use crate::*;

#[derive(Clone)]
pub struct LanguageInfo {
    pub command: String,
    pub args: Vec<String>,

    /// A list of paths to look for when finding the root file
    /// When empty, PWD is used as the root of the workspace
    pub roots: Vec<String>,
}

impl LanguageInfo {
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
    pub client_map: HashMap<String, LspClient<ChildStdin>>,

    pub lang_info_map: HashMap<String, LanguageInfo>,

    pub ext_map: HashMap<String, String>,
}

impl LspManager {
    pub fn register_language(
        &mut self,
        name: impl ToString,
        exts: impl IntoIterator<Item = impl ToString>,
        language_info: LanguageInfo,
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
