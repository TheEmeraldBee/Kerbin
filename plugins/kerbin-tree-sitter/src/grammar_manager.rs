use std::{collections::HashMap, sync::Arc};

use kerbin_core::{kerbin_macros::State, *};
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
    /// A list of extensions that map to normalized language names (Case-Insensitive)
    pub ext_map: HashMap<String, String>,

    /// A map of normalized languages to their definitions
    pub lang_map: HashMap<String, GrammarDefinition>,

    /// A map of normalized languages to their **possibly** loaded grammars
    pub loaded_grammars: HashMap<String, Arc<Grammar>>,

    pub query_map: HashMap<String, HashMap<String, Arc<Query>>>,
}

impl GrammarManager {
    /// Registers all handlers for each extension in the map
    pub async fn register_extension_handlers(&self, state: &mut State) {
        for ext in self.ext_map.keys() {
            state
                .on_hook(hooks::UpdateFiletype::new(ext))
                .system(crate::state::open_files)
                .system(crate::state::update_trees)
                .system(crate::highlighter::highlight_file);
        }
    }

    /// Iterates through all grammars, attempting to locate their installation
    /// Attempts to install them if possible, logging the results
    ///
    /// Never returns an error, will log to console if it fails
    pub async fn install_all_grammars(&self, state: &State) {
        let config_path = state.lock_state::<ConfigFolder>().await.0.clone();

        let mut to_load = vec![];

        for grammar in self.lang_map.values() {
            let lib_paths = grammar.get_file_paths(&config_path);

            if find_library(&lib_paths).is_none() {
                // Clone to allow for threading
                to_load.push(grammar.clone());
            }
        }

        let log = state.lock_state::<LogSender>().await.clone();

        if to_load.is_empty() {
            log.low(
                "tree-sitter::install_all_grammars",
                "All grammars already installed",
            );
        }

        for grammar in to_load {
            let log = log.clone();
            let grammar_name = grammar.name.clone();
            let config_path = config_path.clone();

            tokio::task::spawn_blocking(move || {
                match install_language(format!("{config_path}/runtime/grammars").into(), grammar) {
                    Ok(_) => {
                        log.low(
                            "tree-sitter::install_language",
                            format!("Grammar for {grammar_name} successfully installed!"),
                        );
                    }
                    Err(e) => {
                        log.critical(
                            "tree-sitter::install_language",
                            format!(
                                "Failed to install language {} due to error: {}",
                                grammar_name, e
                            ),
                        );
                    }
                }
            });
        }
    }

    /// Creates the Manager by loading in a list of grammar entries
    /// When it fails, it returns what it got valid still, allowing for a recoverable state
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

                    for ext in &def.exts {
                        ret.ext_map.insert(ext.to_lowercase(), normalized.clone());
                    }

                    ret.lang_map.insert(normalized, def);
                }
                GrammarEntry::Alias {
                    base_lang,
                    new_name,
                    exts,
                } => aliases.push((base_lang, new_name, exts)),
            }
        }

        // all entries are created, lets build the aliases now
        for (lang, new, exts) in aliases {
            let normalized_lang = normalize_lang_name(&lang);
            let normalized_new = normalize_lang_name(&new);

            let Some(mut new_grammar) = ret.lang_map.get(&normalized_lang).cloned() else {
                return Err((
                    ret,
                    GrammarManagerError::MissingDefinition { lang: lang.clone() },
                ));
            };

            new_grammar.exts = exts.clone();

            ret.lang_map.insert(normalized_new.clone(), new_grammar);

            for ext in exts {
                ret.ext_map
                    .insert(ext.to_lowercase(), normalized_new.clone());
            }
        }

        Ok(ret)
    }

    /// Translates an extension into a normalized language string, returning None if non-existent
    pub fn ext_to_lang(&self, ext: &str) -> Option<&str> {
        self.ext_map.get(ext).map(|x| x.as_str())
    }

    /// Attempts to return a grammar, attempting to load it if it isn't already
    /// Accepts any variant of the language name (with -, _, or .)
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

    /// Gets a query set (main query + injected queries) for all queries in a language
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

    /// Gets all possible query file paths for a language, trying all variants
    fn get_query_paths(
        &self,
        config_path: &str,
        normalized_lang: &str,
        query_name: &str,
    ) -> Vec<String> {
        let mut paths = Vec::new();

        // Get all name variants (with -, _, .)
        let variants = get_name_variants(normalized_lang);

        for variant in variants {
            paths.push(format!(
                "{}/runtime/queries/{}/{}.scm",
                config_path, variant, query_name
            ));
            paths.push(format!(
                "{}/runtime/grammars/tree-sitter-{}/queries/{}.scm",
                config_path, variant, query_name
            ));
        }

        paths
    }

    /// Gets or loads a query for a specific language
    /// Accepts any variant of the language name (with -, _, or .)
    pub fn get_query(
        &mut self,
        config_path: &str,
        lang: &str,
        query_name: &str,
    ) -> Option<Arc<Query>> {
        let normalized = normalize_lang_name(lang);

        // Check if query already exists for this language
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

        // Get the grammar (may need to load it)
        let grammar = self.get_grammar(config_path, &normalized).ok()?;

        // Try to load the query from filesystem, checking all variants
        let query_paths = self.get_query_paths(config_path, &normalized, query_name);

        let query_source = query_paths
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())?;

        let query = Query::new(&grammar.lang, &query_source).ok()?;

        // Insert into query_map using normalized name
        self.query_map
            .entry(normalized.clone())
            .or_default()
            .insert(query_name.to_string(), Arc::new(query));

        // Return reference to the inserted query
        self.query_map.get(&normalized)?.get(query_name).cloned()
    }
}

/// Gets all possible variants of a name with -, _, and .
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
