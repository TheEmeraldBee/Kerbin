use kerbin_core::kerbin_macros::State;
use kerbin_core::*;
use libloading::{Library, Symbol};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tree_sitter::{Language, Query};

/// Manages loading and caching Tree-sitter grammars from shared libraries.
#[derive(State)]
pub struct GrammarManager {
    /// Base path where grammars are stored, e.g., `runtime/grammars`
    base_path: PathBuf,

    /// Maps a grammar name to a map of its loaded queries by name (e.g., "highlight").
    loaded_queries: HashMap<String, HashMap<String, Arc<Query>>>,

    /// Maps a grammar name (e.g., "rust") to its loaded Language object.
    loaded_grammars: HashMap<String, Language>,

    /// Maps a file extension (e.g., "rs") to a grammar name ("rust").
    pub extension_map: HashMap<String, String>,
}

impl Default for GrammarManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GrammarManager {
    pub fn new() -> Self {
        Self {
            base_path: ["runtime", "grammars"].iter().collect(),
            loaded_grammars: HashMap::new(),
            loaded_queries: HashMap::new(),
            extension_map: HashMap::new(),
        }
    }

    /// Register an extension to a given language.
    pub fn register_extension(&mut self, ext: impl ToString, lang: impl ToString) {
        self.extension_map.insert(ext.to_string(), lang.to_string());
    }

    /// Gets a language for a given file extension, loading it if necessary.
    pub fn get_language_for_ext(&mut self, extension: &str) -> Option<Language> {
        let lang_name = self.extension_map.get(extension).cloned()?;
        self.get_language(&lang_name)
    }

    /// Gets a language and its highlight query for a given file extension.
    pub fn get_language_and_query_for_ext(
        &mut self,
        extension: &str,
    ) -> Option<(Language, Option<Arc<Query>>)> {
        let lang_name = self.extension_map.get(extension)?.clone();
        let language = self.get_language(&lang_name)?;
        let query = self.get_query(&lang_name, "highlight");

        Some((language, query))
    }

    /// Gets a query for a given language by name (e.g., "highlight", "indent").
    /// It loads and caches the query if it hasn't been loaded yet.
    pub fn get_query(&mut self, lang_name: &str, query_name: &str) -> Option<Arc<Query>> {
        let language = self.get_language(lang_name)?;

        if let Some(queries) = self.loaded_queries.get(lang_name)
            && let Some(query) = queries.get(query_name)
        {
            return Some(query.clone());
        }

        let query_filename = if query_name.ends_with("s") {
            format!("{}.scm", query_name.to_lowercase())
        } else {
            format!("{}s.scm", query_name.to_lowercase())
        };

        let query_path = self.base_path.join(format!(
            "tree-sitter-{}/queries/{}",
            lang_name, query_filename
        ));

        if let Ok(query_source) = std::fs::read_to_string(query_path) {
            let query = match Query::new(&language, &query_source) {
                Ok(q) => Arc::new(q),
                Err(e) => {
                    tracing::error!(
                        "Failed to parse query file for '{}' (query: {}): {:?}",
                        lang_name,
                        query_name,
                        e
                    );
                    return None;
                }
            };

            self.loaded_queries
                .entry(lang_name.to_string())
                .or_default()
                .insert(query_name.to_string(), query.clone());

            Some(query)
        } else {
            None
        }
    }

    /// Loads a grammar by its name (e.g., "rust").
    pub fn get_language(&mut self, name: &str) -> Option<Language> {
        if let Some(lang) = self.loaded_grammars.get(name) {
            return Some(lang.clone());
        }

        // Construct the path to the shared library.
        // e.g., runtime/grammars/tree-sitter-rust/
        let lib_path = self.base_path.join(format!("tree-sitter-{name}"));
        let lib_filename = format!("{}.so", name.replace("-", "_"));

        let lib_file = lib_path.join(lib_filename);

        unsafe {
            // Load the shared library.
            let library = Library::new(&lib_file).ok()?;
            // The symbol name is always `language`.
            let language_func: Symbol<unsafe extern "C" fn() -> Language> = library
                .get(format!("tree_sitter_{name}\0").as_bytes())
                .ok()?;

            let language = language_func();

            // The library must be kept alive, so we leak it.
            std::mem::forget(library);

            self.loaded_grammars
                .insert(name.to_string(), language.clone());
            Some(language)
        }
    }
}
