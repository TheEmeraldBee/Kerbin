use kerbin_core::*;
use serde_json;

use crate::grammar::{GrammarDefinition, GrammarInstallDefinition, normalize_lang_name};
use crate::grammar_manager::GrammarManager;

fn tokens_to_strings(tokens: &[Token]) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|t| {
            if let Token::Word(s) = t {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect()
}

#[derive(Debug, Clone, Command)]
pub enum TreeSitterCommand {
    /// Define a tree-sitter grammar with a git source.
    #[command(drop_ident, name = "tree-sitter-define")]
    Define {
        name: String,
        #[command(flag)]
        exts: Vec<Token>,
        #[command(flag)]
        filenames: Option<Vec<Token>>,
        #[command(flag)]
        url: String,
        #[command(flag)]
        sub_dir: Option<String>,
        #[command(flag)]
        build_name: Option<String>,
    },

    /// Create an alias pointing an existing grammar at a new language name.
    #[command(drop_ident, name = "tree-sitter-alias")]
    Alias {
        base_lang: String,
        new_name: String,
        #[command(flag)]
        exts: Option<Vec<Token>>,
        #[command(flag)]
        filenames: Option<Vec<Token>>,
    },
}

fn register_ts_hook(state: &mut State, filetype: &str) {
    state
        .on_hook(hooks::UpdateFiletype::new(filetype))
        .system(crate::state::open_files)
        .system(crate::state::update_trees)
        .system(crate::highlighter::highlight_file)
        .system(crate::locals::update_locals);
}

#[async_trait::async_trait]
impl Command<State> for TreeSitterCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            TreeSitterCommand::Define {
                name,
                exts,
                filenames,
                url,
                sub_dir,
                build_name,
            } => {
                let normalized = normalize_lang_name(name);
                let ext_strings = tokens_to_strings(exts);
                let filename_strings = filenames
                    .as_deref()
                    .map(tokens_to_strings)
                    .unwrap_or_default();
                let def = GrammarDefinition {
                    name: name.clone(),
                    exts: ext_strings.clone(),
                    entry: None,
                    location: None,
                    install: Some(GrammarInstallDefinition {
                        url: url.clone(),
                        sub_dir: sub_dir.clone(),
                        build_name: build_name.clone(),
                    }),
                };

                // Register grammar definition
                state
                    .lock_state::<GrammarManager>()
                    .await
                    .lang_map
                    .insert(normalized.clone(), def);

                // Register filetype + extensions + explicit filenames in central registry
                {
                    let mut registry = state.lock_state::<FiletypeRegistry>().await;
                    registry.register(&normalized, "tree-sitter");
                    for ext in &ext_strings {
                        registry.register_ext(ext.to_lowercase(), &normalized);
                    }
                    for filename in &filename_strings {
                        registry.register_filename(filename, &normalized);
                    }
                }

                // Register the hook once on the filetype name
                register_ts_hook(state, &normalized);

                // Load package.json detection metadata from installed grammar
                let config_path = state.lock_state::<ConfigFolder>().await.0.clone();
                let pkg_path = format!(
                    "{}/runtime/grammars/tree-sitter-{}/package.json",
                    config_path, name
                );
                if let Ok(src) = std::fs::read_to_string(&pkg_path) {
                    if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&src) {
                        if let Some(configs) = pkg["tree-sitter"].as_array() {
                            let mut registry = state.lock_state::<FiletypeRegistry>().await;
                            for cfg in configs {
                                if let Some(types) = cfg["file-types"].as_array() {
                                    for ft in types {
                                        let Some(s) = ft.as_str() else { continue };
                                        if s.starts_with('.')
                                            || s.chars().any(|c| c.is_uppercase())
                                        {
                                            registry.register_filename(s, &normalized);
                                        } else {
                                            registry
                                                .register_ext(s.to_lowercase(), &normalized);
                                        }
                                    }
                                }
                                if let Some(pattern) = cfg["first-line-match"].as_str() {
                                    registry.register_first_line(pattern, &normalized);
                                }
                            }
                        }
                    }
                }
            }

            TreeSitterCommand::Alias {
                base_lang,
                new_name,
                exts,
                filenames,
            } => {
                let normalized_base = normalize_lang_name(base_lang);
                let normalized_new = normalize_lang_name(new_name);
                let ext_strings = exts.as_deref().map(tokens_to_strings).unwrap_or_default();
                let filename_strings = filenames
                    .as_deref()
                    .map(tokens_to_strings)
                    .unwrap_or_default();

                let mut manager = state.lock_state::<GrammarManager>().await;
                let Some(mut new_def) = manager.lang_map.get(&normalized_base).cloned() else {
                    drop(manager);
                    state.lock_state::<LogSender>().await.critical(
                        "tree-sitter::alias",
                        format!(
                            "Cannot alias '{}' → '{}': base grammar not yet defined",
                            base_lang, new_name
                        ),
                    );
                    return false;
                };

                new_def.exts = ext_strings.clone();
                manager.lang_map.insert(normalized_new.clone(), new_def);
                drop(manager);

                // Register alias filetype + extensions + explicit filenames
                {
                    let mut registry = state.lock_state::<FiletypeRegistry>().await;
                    registry.register(&normalized_new, "tree-sitter");
                    for ext in &ext_strings {
                        registry.register_ext(ext.to_lowercase(), &normalized_new);
                    }
                    for filename in &filename_strings {
                        registry.register_filename(filename, &normalized_new);
                    }
                }

                register_ts_hook(state, &normalized_new);
            }
        }
        false
    }
}
