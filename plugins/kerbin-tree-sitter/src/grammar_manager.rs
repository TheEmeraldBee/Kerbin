use std::{collections::HashMap, sync::Arc};

use kerbin_core::{kerbin_macros::State, *};
use tree_sitter::Query;

use crate::{
    grammar::{Grammar, GrammarDefinition, GrammarEntry, GrammarLoadError},
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
    /// A list of extensions that map to language names (Case-Insensitive)
    pub ext_map: HashMap<String, String>,

    /// A map of languages to their definitions
    pub lang_map: HashMap<String, GrammarDefinition>,

    /// A map of languages to their **possibly** loaded grammars
    pub loaded_grammars: HashMap<String, Arc<Grammar>>,

    pub query_map: HashMap<String, HashMap<String, Arc<Query>>>,
}

impl GrammarManager {
    /// Registers all handlers for each extension in the map
    pub async fn register_extension_handlers(&self, state: &mut State) {
        for ext in self.ext_map.keys() {
            state
                .lock_state::<LogSender>()
                .await
                .critical("tree-sitter::register_handlers", ext);
            state
                .on_hook(hooks::UpdateFiletype::new(ext))
                .system(crate::state::open_files)
                .system(crate::state::update_trees)
                .system(crate::highlighter::highlight_file);
        }
    }

    /// Creates the Manager by loading in a list of grammar entries
    /// When it fails, it returns what it got valid still, allowing for a recoverable state
    pub fn from_definitions(
        entries: Vec<GrammarEntry>,
    ) -> Result<Self, (Self, GrammarManagerError)> {
        let mut ret = Self::default();

        let mut aliases = vec![];

        for entry in entries {
            match entry {
                GrammarEntry::Definition(def) => {
                    for ext in &def.exts {
                        ret.ext_map.insert(ext.to_lowercase(), def.name.clone());
                    }

                    ret.lang_map.insert(def.name.clone(), def);
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
            let Some(mut new_grammar) = ret.lang_map.get(&lang).cloned() else {
                return Err((
                    ret,
                    GrammarManagerError::MissingDefinition { lang: lang.clone() },
                ));
            };

            new_grammar.exts = exts.clone();

            ret.lang_map.insert(new, new_grammar);

            for ext in exts {
                ret.ext_map.insert(ext.to_lowercase(), lang.clone());
            }
        }

        Ok(ret)
    }

    /// Translates an extension into a language string, returning None if non-existant
    pub fn ext_to_lang(&self, ext: &str) -> Option<&str> {
        self.ext_map.get(ext).map(|x| x.as_str())
    }

    /// Attempts to return a grammar, attempting to load it if it isn't already
    pub fn get_grammar(
        &mut self,
        config_path: &str,
        lang: &str,
    ) -> Result<Arc<Grammar>, GrammarManagerError> {
        if self.loaded_grammars.contains_key(lang) {
            return Ok(self
                .loaded_grammars
                .get(lang)
                .cloned()
                .expect("Grammar just checked for existing"));
        }

        // Not found, load it here
        let def = self
            .lang_map
            .get(lang)
            .ok_or(GrammarManagerError::MissingDefinition {
                lang: lang.to_string(),
            })?;

        let grammar = Grammar::from_def(config_path, def)?;

        self.loaded_grammars
            .insert(lang.to_string(), Arc::new(grammar));
        Ok(self
            .loaded_grammars
            .get(lang)
            .cloned()
            .expect("Just inserted language"))
    }

    /// Gets a query set (main query + injected queries) for all queries in a language
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

    /// Gets or loads a query for a specific language
    pub fn get_query(
        &mut self,
        config_path: &str,
        lang: &str,
        query_name: &str,
    ) -> Option<Arc<Query>> {
        // Check if query already exists for this language
        if self.query_map.contains_key(lang)
            && self
                .query_map
                .get(lang)
                .expect("Lang was just checked to exist")
                .contains_key(query_name)
        {
            return Some(
                self.query_map
                    .get(lang)
                    .expect("Lang was just checked to exist")
                    .get(query_name)
                    .cloned()
                    .expect("Query was just checked to exist"),
            );
        }

        // Get the grammar (may need to load it)
        let grammar = self.get_grammar(config_path, lang).ok()?;

        // Try to load the query from filesystem
        let query_paths = [
            format!(
                "{}/runtime/grammars/{}/queries/{}.scm",
                config_path, grammar.name, query_name
            ),
            format!(
                "{}/runtime/queries/{}/{}.scm",
                config_path, grammar.name, query_name
            ),
        ];

        let query_source = query_paths
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())?;

        let query = Query::new(&grammar.lang, &query_source).ok()?;

        // Insert into query_map
        self.query_map
            .entry(lang.to_string())
            .or_insert_with(HashMap::new)
            .insert(query_name.to_string(), Arc::new(query));

        // Return reference to the inserted query
        self.query_map.get(lang)?.get(query_name).cloned()
    }
}
