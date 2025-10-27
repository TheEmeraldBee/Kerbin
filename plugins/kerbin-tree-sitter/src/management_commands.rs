use kerbin_core::{kerbin_macros::Command, *};
use tokio::task::JoinSet;

use crate::{GrammarManager, grammar::GrammarInstallConfig};

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ConfigEntry {
    Language {
        #[serde(rename = "url")]
        git_url: String,

        lang_name: String,
        #[serde(rename = "exts", default)]
        file_extensions: Vec<String>,

        sub_dir: Option<String>,
        alias: Option<String>,
    },
    Alias {
        base_name: String,
        new_name: String,

        #[serde(rename = "exts", default)]
        file_extensions: Vec<String>,
    },
}

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
    /// Installs a list of default tree-sitter languages defined in your configuration.
    ///
    /// Look at the `tree-sitter-grammars.toml` in the default config for context
    TSInstallAllGrammars,
}

#[async_trait::async_trait]
impl Command for TSManagementCommands {
    async fn apply(&self, state: &mut State) -> bool {
        let log = state.lock_state::<LogSender>().await;
        match self {
            Self::TSInstallGrammar {
                git_url,
                lang_name,
                ext: _,
            } => {
                let base_path = state.lock_state::<GrammarManager>().await.base_path.clone();
                let log_clone = log.clone();
                let git_url = git_url.clone();
                let lang_name_clone = lang_name.clone();
                let lang_name = lang_name.clone();

                // Spawn installation in the background
                tokio::spawn(async move {
                    log_clone.low(
                        "tree-sitter::install_language",
                        format!("Starting installation of '{}'", lang_name),
                    );

                    let config = GrammarInstallConfig {
                        base_path,
                        git_url,
                        lang_name: lang_name.clone(),
                        sub_dir: None,
                        special_rename: None,
                    };

                    tokio::task::spawn_blocking(move || {
                        match GrammarManager::install_language(config) {
                            Ok(_) => {
                                log_clone.low(
                                    "tree-sitter::install_language",
                                    format!("Successfully installed '{}'", lang_name),
                                );
                            }
                            Err(e) => {
                                log_clone.critical(
                                    "tree-sitter::install_language",
                                    format!("Failed to install language '{}': {}", lang_name, e),
                                );
                            }
                        }
                    })
                    .await
                    .ok();
                });

                log.low(
                    "tree-sitter::install_language",
                    format!(
                        "Installation of '{}' started in background",
                        lang_name_clone
                    ),
                );

                // Return immediately
                true
            }

            Self::TSInstallAllGrammars => {
                let base_path = state.lock_state::<GrammarManager>().await.base_path.clone();
                let plugin_conf = state.lock_state::<PluginConfig>().await;

                let entries = match plugin_conf.get::<Vec<ConfigEntry>>("tree-sitter-grammars") {
                    None => {
                        log.critical(
                            "tree-sitter::install_all",
                            "No grammars defined in config under table tree-sitter-grammars",
                        );
                        return false;
                    }
                    Some(Err(e)) => {
                        log.critical(
                            "tree-sitter::install_all",
                            format!("malformed config in tree-sitter-grammars: {e}"),
                        );
                        return false;
                    }
                    Some(Ok(t)) => t,
                };

                let log_clone = log.clone();

                // Spawn the entire installation process in the background
                tokio::spawn(async move {
                    let mut join_set = JoinSet::new();

                    log_clone.low(
                        "tree-sitter::install_all",
                        "Starting default grammar installation in background".to_string(),
                    );

                    for entry in entries {
                        match entry {
                            ConfigEntry::Language {
                                git_url,
                                lang_name,
                                file_extensions: _,
                                sub_dir,
                                alias,
                            } => {
                                let config = GrammarInstallConfig {
                                    base_path: base_path.clone(),
                                    git_url,
                                    lang_name: lang_name.clone(),
                                    sub_dir,
                                    special_rename: alias,
                                };

                                let log_for_task = log_clone.clone();
                                let lang_name_clone = lang_name.clone();

                                join_set.spawn_blocking(move || {
                                    log_for_task.low(
                                        "tree-sitter::install_language",
                                        format!("Starting installation of '{}'", lang_name_clone),
                                    );

                                    match GrammarManager::install_language(config) {
                                        Ok(name) => Ok(name),
                                        Err(e) => Err((lang_name_clone, e)),
                                    }
                                });
                            }
                            _ => {}
                        }
                    }

                    // Wait for any remaining parallel tasks
                    while let Some(result) = join_set.join_next().await {
                        match result {
                            Ok(Ok(lang_name)) => {
                                log_clone.low(
                                    "tree-sitter::install_language",
                                    format!("Successfully installed '{}'", lang_name),
                                );
                            }
                            Ok(Err((lang_name, e))) => {
                                log_clone.critical(
                                    "tree-sitter::install_language",
                                    format!("Failed to install language '{}': {}", lang_name, e),
                                );
                            }
                            Err(e) => {
                                log_clone.critical(
                                    "tree-sitter::install_language",
                                    format!("Task panicked: {:?}", e),
                                );
                            }
                        }
                    }

                    log_clone.low(
                        "tree-sitter::install_all",
                        "Completed default grammar installation".to_string(),
                    );
                });

                log.low(
                    "tree-sitter::install_all",
                    "Default grammar installation started in background".to_string(),
                );

                // Return immediately
                true
            }
        }
    }
}
