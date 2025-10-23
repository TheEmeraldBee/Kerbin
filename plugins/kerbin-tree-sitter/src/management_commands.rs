use kerbin_core::{kerbin_macros::Command, *};
use tokio::task::JoinSet;

use crate::{GrammarManager, grammar::GrammarInstallConfig};

const DEFAULT_GRAMMARS_LIST: &str = include_str!("../default_grammars.txt");

#[derive(Command, Debug, Clone)]
pub enum TSManagementCommands {
    #[command(drop_ident, name = "ts_install_lang")]
    /// Installs a new tree-sitter language from a Git URL and registers an extension.
    TSInstallGrammar {
        #[command(type_name = "String", name = "git_url")]
        git_url: String,
        #[command(type_name = "String", name = "lang_name")]
        lang_name: String,
        #[command(type_name = "String", name = "ext")]
        ext: String,
    },

    #[command(drop_ident, name = "ts_install_defaults")]
    /// Installs a list of default tree-sitter languages defined in a static file in parallel.
    TSInstallDefaultGrammars,
}

#[async_trait::async_trait]
impl Command for TSManagementCommands {
    async fn apply(&self, state: &mut State) -> bool {
        let log = state.lock_state::<LogSender>().await.unwrap();
        match self {
            Self::TSInstallGrammar {
                git_url,
                lang_name,
                ext,
            } => {
                let mut grammars = state.lock_state::<GrammarManager>().await.unwrap();

                let config = GrammarInstallConfig {
                    base_path: grammars.base_path.clone(),
                    git_url: git_url.clone(),
                    lang_name: lang_name.clone(),
                    sub_dir: None,
                    special_rename: None,
                };

                match GrammarManager::install_language(config) {
                    Ok(_) => {
                        grammars.register_extension(ext.clone(), lang_name);
                        log.low(
                            "tree-sitter::install_language",
                            format!(
                                "Tree-sitter language '{lang_name}' installed and registered for '.{ext}'"
                            )
                        );
                        true
                    }
                    Err(e) => {
                        log.critical(
                            "tree-sitter::install_language",
                            format!("Failed to install tree-sitter language '{lang_name}': {e}"),
                        );
                        false
                    }
                }
            }

            Self::TSInstallDefaultGrammars => {
                let base_path = state
                    .lock_state::<GrammarManager>()
                    .await
                    .unwrap()
                    .base_path
                    .clone();

                let mut join_set = JoinSet::new();
                let mut parallel = true;

                let mut all_succeeded = true;

                for line in DEFAULT_GRAMMARS_LIST.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }

                    if line == "stop-concurrent" {
                        parallel = false;

                        let mut new_join_set = JoinSet::new();

                        std::mem::swap(&mut new_join_set, &mut join_set);

                        let results = new_join_set.join_all().await;

                        let mut grammars = state.lock_state::<GrammarManager>().await.unwrap();

                        for result in results {
                            match result {
                                Ok((lang_name, ext)) => {
                                    grammars.register_extension(ext, &lang_name);
                                }
                                Err((lang_name, e)) => {
                                    log.critical(
                                        "tree-sitter::install_language",
                                        format!(
                                            "Failed to install language '{}': {}",
                                            lang_name, e
                                        ),
                                    );
                                    all_succeeded = false;
                                }
                            }
                        }

                        continue;
                    }

                    if line == "start-concurrent" {
                        parallel = true;

                        continue;
                    }

                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() < 2 {
                        log.high(
                            "tree-sitter::install_language",
                            format!("Skipping malformed grammar line: {}", line),
                        );
                        continue;
                    }

                    let git_url = parts[0].to_string();
                    let lang_name = parts[1].to_string();
                    let ext = parts.get(2).unwrap_or(&lang_name.as_str()).to_string();
                    let sub_dir = parts.get(3).map(|s| s.to_string());
                    let special_rename = parts.get(4).map(|s| s.to_string());

                    let config = GrammarInstallConfig {
                        base_path: base_path.clone(),
                        git_url,
                        lang_name: lang_name.clone(),
                        sub_dir,
                        special_rename,
                    };

                    if parallel {
                        join_set.spawn_blocking(move || {
                            match GrammarManager::install_language(config) {
                                Ok(name) => Ok((name, ext)),
                                Err(e) => Err((lang_name, e)),
                            }
                        });
                    } else {
                        let mut grammars = state.lock_state::<GrammarManager>().await.unwrap();

                        match GrammarManager::install_language(config) {
                            Ok(lang_name) => {
                                grammars.register_extension(ext, &lang_name);
                            }
                            Err(e) => {
                                log.critical(
                                    "tree-sitter::install_language",
                                    format!("Failed to install language '{}': {}", lang_name, e),
                                );
                                all_succeeded = false;
                            }
                        }
                    }
                }

                let results = join_set.join_all().await;

                let mut grammars = state.lock_state::<GrammarManager>().await.unwrap();

                for result in results {
                    match result {
                        Ok((lang_name, ext)) => {
                            grammars.register_extension(ext.clone(), &lang_name);
                        }
                        Err((lang_name, e)) => {
                            log.critical(
                                "tree-sitter::install_language",
                                format!("Failed to install language '{}': {}", lang_name, e),
                            );
                            all_succeeded = false;
                        }
                    }
                }

                all_succeeded
            }
        }
    }
}
