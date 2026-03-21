use kerbin_core::*;

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
    },
}

#[async_trait::async_trait]
impl Command for TreeSitterCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            TreeSitterCommand::Define {
                name,
                exts,
                url,
                sub_dir,
                build_name,
            } => {
                let normalized = normalize_lang_name(name);
                let ext_strings = tokens_to_strings(exts);
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

                {
                    let mut manager = state.lock_state::<GrammarManager>().await;
                    for ext in &ext_strings {
                        manager
                            .ext_map
                            .insert(ext.to_lowercase(), normalized.clone());
                    }
                    manager.lang_map.insert(normalized, def);
                }

                for ext in ext_strings {
                    state
                        .on_hook(hooks::UpdateFiletype::new(&ext))
                        .system(crate::state::open_files)
                        .system(crate::state::update_trees)
                        .system(crate::highlighter::highlight_file)
                        .system(crate::locals::update_locals);
                }
            }

            TreeSitterCommand::Alias {
                base_lang,
                new_name,
                exts,
            } => {
                let normalized_base = normalize_lang_name(base_lang);
                let normalized_new = normalize_lang_name(new_name);
                let ext_strings = exts.as_deref().map(tokens_to_strings).unwrap_or_default();

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
                for ext in &ext_strings {
                    manager
                        .ext_map
                        .insert(ext.to_lowercase(), normalized_new.clone());
                }
                drop(manager);

                for ext in ext_strings {
                    state
                        .on_hook(hooks::UpdateFiletype::new(&ext))
                        .system(crate::state::open_files)
                        .system(crate::state::update_trees)
                        .system(crate::highlighter::highlight_file)
                        .system(crate::locals::update_locals);
                }
            }
        }
        false
    }
}
