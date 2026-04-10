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
    /// Define a tree-sitter grammar and the language names it serves.
    #[command(drop_ident, name = "tree_sitter_define")]
    Define {
        name: String,
        /// Language names this grammar handles (e.g. [rust] or [typescript, typescriptreact])
        #[command(flag)]
        langs: Option<Vec<Token>>,
        #[command(flag)]
        url: String,
        #[command(flag)]
        sub_dir: Option<String>,
        #[command(flag)]
        build_name: Option<String>,
    },
}

fn register_ts_hook(state: &mut State, lang: &str) {
    if state.has_hook_system(hooks::UpdateFiletype::new(lang), "tree-sitter::open_files") {
        return;
    }
    state
        .on_hook(hooks::UpdateFiletype::new(lang))
        .system_named("tree-sitter::open_files", crate::state::open_files)
        .system_named("tree-sitter::update_trees", crate::state::update_trees)
        .system_named("tree-sitter::highlight_file", crate::highlighter::highlight_file)
        .system_named("tree-sitter::update_locals", crate::locals::update_locals);
}

#[async_trait::async_trait]
impl Command<State> for TreeSitterCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            TreeSitterCommand::Define {
                name,
                langs,
                url,
                sub_dir,
                build_name,
            } => {
                let grammar_name = normalize_lang_name(name);
                let lang_strings = langs
                    .as_deref()
                    .map(tokens_to_strings)
                    .unwrap_or_default();

                let def = GrammarDefinition {
                    name: name.clone(),
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
                    manager.grammar_map.insert(grammar_name.clone(), def);
                    for lang in &lang_strings {
                        let normalized_lang = normalize_lang_name(lang);
                        manager
                            .lang_to_grammar
                            .insert(normalized_lang, grammar_name.clone());
                    }
                }

                for lang in &lang_strings {
                    register_ts_hook(state, lang);
                }
            }
        }
        false
    }
}
