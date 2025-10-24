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
        let log = state.lock_state::<LogSender>().await;
        match self {
            Self::TSInstallGrammar {
                git_url,
                lang_name,
                ext,
            } => {
                let mut grammars = state.lock_state::<GrammarManager>().await;

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
                let base_path = state.lock_state::<GrammarManager>().await.base_path.clone();
                let log_clone = log.clone();

                // Spawn the entire installation process in the background
                tokio::spawn(async move {
                    let mut join_set = JoinSet::new();
                    let mut parallel = true;

                    log_clone.low(
                        "tree-sitter::install_defaults",
                        "Starting default grammar installation in background".to_string(),
                    );

                    for line in DEFAULT_GRAMMARS_LIST.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }

                        if line == "stop-concurrent" {
                            parallel = false;

                            // Wait for all current parallel tasks to complete
                            let mut new_join_set = JoinSet::new();
                            std::mem::swap(&mut new_join_set, &mut join_set);

                            while let Some(result) = new_join_set.join_next().await {
                                match result {
                                    Ok(Ok((lang_name, ext))) => {
                                        log_clone.low(
                                            "tree-sitter::install_language",
                                            format!(
                                                "Successfully installed '{}' for extension '.{}'",
                                                lang_name, ext
                                            ),
                                        );
                                    }
                                    Ok(Err((lang_name, e))) => {
                                        log_clone.critical(
                                            "tree-sitter::install_language",
                                            format!(
                                                "Failed to install language '{}': {}",
                                                lang_name, e
                                            ),
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

                            continue;
                        }

                        if line == "start-concurrent" {
                            parallel = true;
                            continue;
                        }

                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() < 2 {
                            log_clone.high(
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

                        let log_for_task = log_clone.clone();
                        let lang_name_clone = lang_name.clone();
                        let ext_clone = ext.clone();

                        if parallel {
                            join_set.spawn_blocking(move || {
                                log_for_task.low(
                                    "tree-sitter::install_language",
                                    format!("Starting installation of '{}'", lang_name_clone),
                                );

                                match GrammarManager::install_language(config) {
                                    Ok(name) => Ok((name, ext_clone)),
                                    Err(e) => Err((lang_name_clone, e)),
                                }
                            });
                        } else {
                            log_for_task.low(
                                "tree-sitter::install_language",
                                format!(
                                    "Starting installation of '{}' (sequential)",
                                    lang_name_clone
                                ),
                            );

                            match GrammarManager::install_language(config) {
                                Ok(_) => {
                                    log_for_task.low(
                                        "tree-sitter::install_language",
                                        format!(
                                            "Successfully installed '{}' for extension '.{}'",
                                            lang_name, ext
                                        ),
                                    );
                                }
                                Err(e) => {
                                    log_for_task.critical(
                                        "tree-sitter::install_language",
                                        format!(
                                            "Failed to install language '{}': {}",
                                            lang_name, e
                                        ),
                                    );
                                }
                            }
                        }
                    }

                    // Wait for any remaining parallel tasks
                    while let Some(result) = join_set.join_next().await {
                        match result {
                            Ok(Ok((lang_name, ext))) => {
                                log_clone.low(
                                    "tree-sitter::install_language",
                                    format!(
                                        "Successfully installed '{}' for extension '.{}'",
                                        lang_name, ext
                                    ),
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
                        "tree-sitter::install_defaults",
                        "Completed default grammar installation".to_string(),
                    );
                });

                log.low(
                    "tree-sitter::install_defaults",
                    "Default grammar installation started in background".to_string(),
                );

                // Return immediately
                true
            }
        }
    }
}
