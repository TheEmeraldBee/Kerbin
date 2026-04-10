use std::{collections::HashMap, sync::Arc};

use kerbin_core::*;
use tree_sitter::Query;

use crate::{
    grammar::{Grammar, GrammarDefinition, GrammarLoadError, find_library, normalize_lang_name},
    grammar_install::install_language,
    state::TreeSitterState,
};

#[derive(thiserror::Error, Debug)]
pub enum GrammarManagerError {
    #[error(transparent)]
    LoadError(#[from] GrammarLoadError),

    #[error("No grammar registered for language '{lang}'")]
    MissingDefinition { lang: String },
}

#[derive(State, Default)]
pub struct GrammarManager {
    /// Grammar name → grammar definition
    pub grammar_map: HashMap<String, GrammarDefinition>,
    pub loaded_grammars: HashMap<String, Arc<Grammar>>,
    pub query_map: HashMap<String, HashMap<String, Arc<Query>>>,
    pub failed_queries: std::collections::HashSet<(String, String)>,
    /// Language name → grammar name (many languages can share one grammar)
    pub lang_to_grammar: HashMap<String, String>,
}

impl GrammarManager {
    pub async fn register_filetype_handlers(&self, state: &mut State) {
        for lang in self.lang_to_grammar.keys() {
            state
                .on_hook(hooks::UpdateFiletype::new(lang))
                .system(crate::state::open_files)
                .system(crate::state::update_trees)
                .system(crate::highlighter::highlight_file)
                .system(crate::locals::update_locals);
        }
    }

    pub async fn install_all_grammars(&self, state: &State) {
        let config_path = state.lock_state::<ConfigFolder>().await.0.clone();
        let log = state.lock_state::<LogSender>().await.clone();

        let mut to_load = vec![];
        let mut already_installed = vec![];

        for grammar in self.grammar_map.values() {
            let lib_paths = grammar.get_file_paths(&config_path);
            if find_library(&lib_paths).is_none() {
                to_load.push(grammar.clone());
            } else {
                already_installed.push(grammar.name.as_str());
            }
        }

        if !already_installed.is_empty() {
            already_installed.sort();
            log.low(
                "tree-sitter::install_all_grammars",
                format!(
                    "{} grammars already installed: {}",
                    already_installed.len(),
                    already_installed.join(", ")
                ),
            );
        }

        if to_load.is_empty() {
            log.low(
                "tree-sitter::install_all_grammars",
                "All grammars already installed, nothing to do",
            );
            return;
        }

        let mut names: Vec<&str> = to_load.iter().map(|g| g.name.as_str()).collect();
        names.sort();
        log.low(
            "tree-sitter::install_all_grammars",
            format!("Installing {} grammars: {}", to_load.len(), names.join(", ")),
        );

        for grammar in to_load {
            let log = log.clone();
            let grammar_name = grammar.name.clone();
            let config_path = config_path.clone();

            tokio::task::spawn_blocking(move || {
                match install_language(format!("{config_path}/runtime/grammars").into(), grammar) {
                    Ok(_) => {
                        log.low(
                            "tree-sitter::install_language",
                            format!("Installed grammar: {grammar_name}"),
                        );
                    }
                    Err(e) => {
                        log.critical(
                            "tree-sitter::install_language",
                            format!("Failed to install grammar {grammar_name}: {e}"),
                        );
                    }
                }
            });
        }
    }

    /// Resolve a language or grammar name to a loaded Grammar.
    ///
    /// Lookup order:
    /// 1. `lang_to_grammar[name]` → grammar_name (explicit language mapping)
    /// 2. `grammar_map[name]` directly (grammar name used as-is, e.g. from injection queries)
    #[allow(clippy::result_large_err)]
    pub fn get_grammar(
        &mut self,
        config_path: &str,
        name: &str,
    ) -> Result<Arc<Grammar>, GrammarManagerError> {
        let normalized_name = normalize_lang_name(name);

        let grammar_name = self
            .lang_to_grammar
            .get(&normalized_name)
            .cloned()
            .unwrap_or_else(|| normalized_name.clone());

        if let Some(grammar) = self.loaded_grammars.get(&grammar_name) {
            return Ok(grammar.clone());
        }

        let def = self
            .grammar_map
            .get(&grammar_name)
            .ok_or_else(|| GrammarManagerError::MissingDefinition {
                lang: name.to_string(),
            })?
            .clone();

        let grammar = Grammar::from_def(config_path, &def)?;
        self.loaded_grammars
            .insert(grammar_name.clone(), Arc::new(grammar));

        Ok(self
            .loaded_grammars
            .get(&grammar_name)
            .cloned()
            .expect("Just inserted"))
    }

    #[allow(clippy::type_complexity)]
    pub fn get_query_set(
        &mut self,
        config_path: &str,
        query_name: &str,
        state: &TreeSitterState,
    ) -> Option<(Arc<Query>, HashMap<String, Arc<Query>>)> {
        let query = self.get_query(config_path, &state.lang, query_name)?;

        let mut injected_queries = HashMap::new();
        for injected in &state.injected_trees {
            if let Some(query) = self.get_query(config_path, &injected.lang, query_name) {
                injected_queries.insert(injected.lang.clone(), query);
            }
        }

        Some((query, injected_queries))
    }

    fn get_query_source(
        &self,
        config_path: &str,
        grammar_name: &str,
        query_name: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> Option<String> {
        if !visited.insert(grammar_name.to_string()) {
            return Some(String::new());
        }

        let paths = self.get_query_paths(config_path, grammar_name, query_name);
        let source = match paths.iter().find_map(|path| std::fs::read_to_string(path).ok()) {
            Some(s) => s,
            None => {
                tracing::debug!(
                    "tree-sitter: no '{query_name}' query file found for '{grammar_name}', checked: {paths:?}"
                );
                return None;
            }
        };

        let mut inherited_langs: Vec<String> = Vec::new();
        for line in source.lines() {
            if let Some(rest) = line.strip_prefix("; inherits:") {
                for lang in rest.trim().split(',') {
                    inherited_langs.push(normalize_lang_name(lang.trim()));
                }
            }
        }

        if inherited_langs.is_empty() {
            return Some(source);
        }

        let mut combined = String::new();
        for inherited in inherited_langs {
            if let Some(parent) =
                self.get_query_source(config_path, &inherited, query_name, visited)
            {
                combined.push_str(&parent);
                combined.push('\n');
            }
        }
        combined.push_str(&source);
        Some(combined)
    }

    fn get_query_paths(
        &self,
        config_path: &str,
        grammar_name: &str,
        query_name: &str,
    ) -> Vec<String> {
        let mut paths = Vec::new();

        let variants = get_name_variants(grammar_name);

        for variant in variants {
            paths.push(format!(
                "{}/runtime/queries/{}/{}.scm",
                config_path, variant, query_name
            ));
            paths.push(format!(
                "{}/runtime/grammars/tree-sitter-{}/queries/{}/{}.scm",
                config_path, variant, variant, query_name
            ));
            paths.push(format!(
                "{}/runtime/grammars/tree-sitter-{}/queries/{}.scm",
                config_path, variant, query_name
            ));
        }

        paths
    }

    pub fn get_query(
        &mut self,
        config_path: &str,
        lang: &str,
        query_name: &str,
    ) -> Option<Arc<Query>> {
        let normalized_lang = normalize_lang_name(lang);

        // Resolve to the actual grammar name for cache keys and file paths
        let grammar_name = self
            .lang_to_grammar
            .get(&normalized_lang)
            .cloned()
            .unwrap_or_else(|| normalized_lang.clone());

        if self
            .failed_queries
            .contains(&(grammar_name.clone(), query_name.to_string()))
        {
            return None;
        }

        if let Some(queries) = self.query_map.get(&grammar_name)
            && let Some(query) = queries.get(query_name) {
                return Some(query.clone());
            }

        let grammar = self.get_grammar(config_path, lang).ok()?;

        let mut visited = std::collections::HashSet::new();
        let query_source =
            self.get_query_source(config_path, &grammar.name, query_name, &mut visited)?;

        let query = match Query::new(&grammar.lang, &query_source) {
            Ok(q) => q,
            Err(e) => {
                tracing::error!(
                    "tree-sitter: failed to compile '{query_name}' query for '{lang}': {e}"
                );
                self.failed_queries
                    .insert((grammar_name, query_name.to_string()));
                return None;
            }
        };

        self.query_map
            .entry(grammar_name.clone())
            .or_default()
            .insert(query_name.to_string(), Arc::new(query));

        self.query_map
            .get(&grammar_name)?
            .get(query_name)
            .cloned()
    }
}

fn get_name_variants(name: &str) -> Vec<String> {
    let mut variants = vec![name.to_string()];

    if name.contains('_') {
        variants.push(name.replace('_', "-"));
        variants.push(name.replace('_', "."));
    }

    variants.sort();
    variants.dedup();
    variants
}
