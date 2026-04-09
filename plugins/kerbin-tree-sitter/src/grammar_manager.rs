use std::{collections::HashMap, sync::Arc};

use kerbin_core::*;
use tree_sitter::Query;

use crate::{
    grammar::{
        Grammar, GrammarDefinition, GrammarEntry, GrammarLoadError, find_library,
        normalize_lang_name,
    },
    grammar_install::install_language,
    state::TreeSitterState,
};

#[derive(thiserror::Error, Debug)]
pub enum GrammarManagerError {
    #[error(transparent)]
    LoadError(#[from] GrammarLoadError),

    #[error("Missing definition for grammar {lang}")]
    MissingDefinition { lang: String },
}

#[derive(State, Default)]
pub struct GrammarManager {
    pub lang_map: HashMap<String, GrammarDefinition>,
    pub loaded_grammars: HashMap<String, Arc<Grammar>>,
    pub query_map: HashMap<String, HashMap<String, Arc<Query>>>,
    pub failed_queries: std::collections::HashSet<(String, String)>,
}

impl GrammarManager {
    pub async fn register_filetype_handlers(&self, state: &mut State) {
        for filetype in self.lang_map.keys() {
            state
                .on_hook(hooks::UpdateFiletype::new(filetype))
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

        for grammar in self.lang_map.values() {
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

    #[allow(clippy::result_large_err)]
    pub fn from_definitions(
        entries: Vec<GrammarEntry>,
    ) -> Result<Self, (Self, GrammarManagerError)> {
        let mut ret = Self::default();

        let mut aliases = vec![];

        for entry in entries {
            match entry {
                GrammarEntry::Definition(def) => {
                    let normalized = normalize_lang_name(&def.name);
                    ret.lang_map.insert(normalized, def);
                }
                GrammarEntry::Alias {
                    base_lang,
                    new_name,
                    exts,
                } => aliases.push((base_lang, new_name, exts)),
            }
        }

        for (lang, new, exts) in aliases {
            let normalized_lang = normalize_lang_name(&lang);
            let normalized_new = normalize_lang_name(&new);

            let Some(mut new_grammar) = ret.lang_map.get(&normalized_lang).cloned() else {
                return Err((
                    ret,
                    GrammarManagerError::MissingDefinition { lang: lang.clone() },
                ));
            };

            new_grammar.exts = exts;
            ret.lang_map.insert(normalized_new, new_grammar);
        }

        Ok(ret)
    }

    pub fn get_grammar(
        &mut self,
        config_path: &str,
        lang: &str,
    ) -> Result<Arc<Grammar>, GrammarManagerError> {
        let normalized = normalize_lang_name(lang);

        if self.loaded_grammars.contains_key(&normalized) {
            return Ok(self
                .loaded_grammars
                .get(&normalized)
                .cloned()
                .expect("Grammar just checked for existing"));
        }

        // Not found, load it here
        let def = self
            .lang_map
            .get(&normalized)
            .ok_or(GrammarManagerError::MissingDefinition {
                lang: lang.to_string(),
            })?;

        let grammar = Grammar::from_def(config_path, def)?;

        self.loaded_grammars
            .insert(normalized.clone(), Arc::new(grammar));
        Ok(self
            .loaded_grammars
            .get(&normalized)
            .cloned()
            .expect("Just inserted language"))
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
        lang: &str,
        query_name: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> Option<String> {
        if !visited.insert(lang.to_string()) {
            return Some(String::new());
        }

        let paths = self.get_query_paths(config_path, lang, query_name);
        let source = match paths.iter().find_map(|path| std::fs::read_to_string(path).ok()) {
            Some(s) => s,
            None => {
                tracing::debug!(
                    "tree-sitter: no '{query_name}' query file found for '{lang}', checked: {paths:?}"
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
        normalized_lang: &str,
        query_name: &str,
    ) -> Vec<String> {
        let mut paths = Vec::new();

        let variants = get_name_variants(normalized_lang);

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
        let normalized = normalize_lang_name(lang);

        if self.failed_queries.contains(&(normalized.clone(), query_name.to_string())) {
            return None;
        }

        if self.query_map.contains_key(&normalized)
            && self
                .query_map
                .get(&normalized)
                .expect("Lang was just checked to exist")
                .contains_key(query_name)
        {
            return Some(
                self.query_map
                    .get(&normalized)
                    .expect("Lang was just checked to exist")
                    .get(query_name)
                    .cloned()
                    .expect("Query was just checked to exist"),
            );
        }

        let grammar = self.get_grammar(config_path, &normalized).ok()?;

        // Use the grammar's own name for path resolution so aliases fall through
        // to the base grammar's query files (e.g. "kerbin" alias → "bash" queries).
        let mut visited = std::collections::HashSet::new();
        let query_source =
            self.get_query_source(config_path, &grammar.name, query_name, &mut visited)?;

        let query = match Query::new(&grammar.lang, &query_source) {
            Ok(q) => q,
            Err(e) => {
                tracing::error!(
                    "tree-sitter: failed to compile '{query_name}' query for '{lang}': {e}"
                );
                self.failed_queries.insert((normalized, query_name.to_string()));
                return None;
            }
        };

        self.query_map
            .entry(normalized.clone())
            .or_default()
            .insert(query_name.to_string(), Arc::new(query));

        self.query_map.get(&normalized)?.get(query_name).cloned()
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
