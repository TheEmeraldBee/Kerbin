use kerbin_core::kerbin_macros::State;
use kerbin_core::*;
use libloading::{Library, Symbol};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tree_sitter::{Language, Query};

pub struct GrammarInstallConfig {
    pub base_path: PathBuf,
    pub git_url: String,
    pub lang_name: String,
    pub sub_dir: Option<String>,
    pub special_rename: Option<String>,
}

/// Manages loading and caching Tree-sitter grammars from shared libraries.
#[derive(State)]
pub struct GrammarManager {
    /// Base path where grammars are stored, e.g., `{config_path}`
    pub(crate) base_path: PathBuf,

    /// Maps a normalized grammar name to a map of its loaded queries by name (e.g., "highlight").
    /// Keys are normalized (c_sharp, not c.sharp or c-sharp)
    loaded_queries: HashMap<String, HashMap<String, Arc<Query>>>,

    /// Maps a normalized grammar name (e.g., "rust", "c_sharp") to its loaded Language object.
    /// Keys are normalized (c_sharp, not c.sharp or c-sharp)
    loaded_grammars: HashMap<String, Language>,

    /// Maps a file extension (e.g., "rs") to a normalized grammar name ("rust", "c_sharp").
    /// Values are normalized for consistency
    pub extension_map: HashMap<String, String>,

    /// Maps a language alias to its parent grammar.
    /// e.g., "tsx" -> "typescript" means tsx inherits typescript's grammar but has its own queries
    language_inheritance: HashMap<String, String>,
}

impl GrammarManager {
    pub fn new(base_path: String) -> Self {
        Self {
            base_path: PathBuf::from(base_path).join("runtime/grammars"),
            loaded_grammars: HashMap::new(),
            loaded_queries: HashMap::new(),
            extension_map: HashMap::new(),
            language_inheritance: HashMap::new(),
        }
    }

    /// Normalizes a language name by replacing `-` and `.` with `_`.
    pub fn normalize_lang_name(name: &str) -> String {
        name.replace("-", "_").replace(".", "_")
    }

    /// Cleans up the grammar directory after a successful build, keeping only the
    /// compiled library and query files.
    fn cleanup_grammar_directory(dir: &Path, lang_name: &str) -> io::Result<()> {
        let normalized = Self::normalize_lang_name(lang_name);
        let essential_files: Vec<String> = vec![
            format!("{}.so", normalized),
            format!("{}.dll", normalized),
            format!("{}.dylib", normalized),
        ];

        let query_dir_name = "queries";
        let query_dir_path = dir.join(query_dir_name);

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

            if path.is_dir() {
                if file_name != query_dir_name {
                    fs::remove_dir_all(path)?;
                }
            } else if !essential_files.contains(&file_name.to_string()) {
                fs::remove_file(path)?;
            }
        }

        let src_queries = dir.join("src").join(query_dir_name);
        if src_queries.exists() && src_queries.is_dir() {
            if query_dir_path.exists() {
                fs::remove_dir_all(&query_dir_path).ok();
            }
            fs::rename(&src_queries, &query_dir_path)?;
            fs::remove_dir(dir.join("src")).ok();
        }

        Ok(())
    }

    /// Installs a language based on the install config
    /// Installs to the config's runtime/grammars path with normalized directory name
    pub fn install_language(config: GrammarInstallConfig) -> Result<String, String> {
        use std::process::Command;

        let repo_name = config
            .git_url
            .split('/')
            .next_back()
            .unwrap_or(&config.lang_name)
            .replace(".git", "");

        // Clone into a separate build directory
        let build_root = config.base_path.join(".build");
        let repo_clone_dir = build_root.join(&repo_name);

        let build_dir = config
            .sub_dir
            .as_ref()
            .map(|sub| repo_clone_dir.join(sub))
            .unwrap_or_else(|| repo_clone_dir.clone());

        // Use normalized name for the final directory
        let normalized_lang = Self::normalize_lang_name(&config.lang_name);
        let final_grammar_dir = config
            .base_path
            .join(format!("tree-sitter-{}", normalized_lang));

        if final_grammar_dir.exists() {
            fs::remove_dir_all(&final_grammar_dir)
                .map_err(|e| format!("Failed to clean up existing grammar directory: {e}"))?;
        }

        let result = (|| {
            if !repo_clone_dir.exists() {
                // Ensure the build root directory exists
                fs::create_dir_all(&build_root)
                    .map_err(|e| format!("Failed to create build directory: {e}"))?;

                tracing::info!("Cloning {} into {:?}", config.git_url, repo_clone_dir);
                let output = Command::new("git")
                    .arg("clone")
                    .arg("--depth")
                    .arg("1")
                    .arg(&config.git_url)
                    .arg(&repo_clone_dir)
                    .output()
                    .map_err(|e| {
                        format!("Failed to run git clone. Is 'git' installed? Error: {e}")
                    })?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(format!(
                        "Git clone failed (Status: {}): {}",
                        output.status, stderr
                    ));
                }
            }

            if !build_dir.exists() {
                return Err(format!(
                    "Cloned repository is missing the required build directory: {:?}",
                    build_dir
                ));
            }

            tracing::info!("Running `tree-sitter build` in {:?}", build_dir);
            let build_output = Command::new("tree-sitter")
                .arg("build")
                .current_dir(&build_dir)
                .output()
                .map_err(|e| format!("Failed to run `tree-sitter build`. Is the `tree-sitter` CLI and a C compiler installed? Error: {e}"))?;

            if !build_output.status.success() {
                let stderr = String::from_utf8_lossy(&build_output.stderr);
                return Err(format!(
                    "`tree-sitter build` failed (Status: {}). Output: {}",
                    build_output.status, stderr
                ));
            }

            let initial_filename_so = format!(
                "{}.so",
                config
                    .special_rename
                    .clone()
                    .unwrap_or(normalized_lang.clone())
            );
            let initial_filename_dll = format!(
                "{}.dll",
                config
                    .special_rename
                    .clone()
                    .unwrap_or(normalized_lang.clone())
            );
            let initial_filename_dylib = format!(
                "{}.dylib",
                config
                    .special_rename
                    .clone()
                    .unwrap_or(normalized_lang.clone())
            );

            let lib_filename_so = format!("{}.so", normalized_lang);
            let lib_filename_dll = format!("{}.dll", normalized_lang);
            let lib_filename_dylib = format!("{}.dylib", normalized_lang);

            let (initial_compiled_lib_name, compiled_lib_name) = [
                (initial_filename_so.clone(), lib_filename_so.clone()),
                (initial_filename_dll.clone(), lib_filename_dll.clone()),
                (initial_filename_dylib.clone(), lib_filename_dylib.clone()),
            ]
            .iter()
            .find(|name| build_dir.join(&name.0).exists())
            .ok_or_else(|| {
                format!(
                    "Build succeeded but shared library was not found ({}|{}|{}).",
                    lib_filename_so, lib_filename_dll, lib_filename_dylib
                )
            })?
            .clone();

            fs::create_dir_all(&final_grammar_dir)
                .map_err(|e| format!("Failed to create final grammar directory: {e}"))?;

            fs::rename(
                build_dir.join(&initial_compiled_lib_name),
                final_grammar_dir.join(&compiled_lib_name),
            )
            .map_err(|e| format!("Failed to move compiled library: {e}"))?;

            let final_query_dir = final_grammar_dir.join("queries");
            let source_query_dirs = [build_dir.join("queries"), build_dir.join("src/queries")];

            let queries_moved = source_query_dirs.iter().any(|source_dir| {
                if source_dir.exists() {
                    if final_query_dir.exists() {
                        fs::remove_dir_all(&final_query_dir).ok();
                    }
                    fs::rename(source_dir, &final_query_dir).is_ok()
                } else {
                    false
                }
            });

            if !queries_moved {
                tracing::warn!(
                    "Could not find queries for {} in {:?} or {:?}. Continuing.",
                    normalized_lang,
                    build_dir.join("queries"),
                    build_dir.join("src/queries")
                );
            }

            Self::cleanup_grammar_directory(&final_grammar_dir, &normalized_lang)
                .map_err(|e| format!("Failed to cleanup build dir {e}"))?;

            Ok(normalized_lang)
        })();

        // Always clean up the build directory after installation (success or failure)
        if repo_clone_dir.exists() {
            tracing::debug!("Cleaning up build directory: {:?}", repo_clone_dir);
            fs::remove_dir_all(&repo_clone_dir).ok();
        }

        result
    }

    /// Register an extension to a given language.
    /// The lang parameter will be normalized automatically (c.sharp -> c_sharp)
    pub fn register_extension(&mut self, ext: impl ToString, lang: impl ToString) {
        let normalized_lang = Self::normalize_lang_name(&lang.to_string());
        self.extension_map.insert(ext.to_string(), normalized_lang);
    }

    /// Register a language that inherits a grammar from another language.
    /// The child will use the parent's grammar but can have its own queries.
    ///
    /// # Example
    /// ```
    /// // tsx inherits typescript's grammar but has its own queries
    /// grammar_manager.register_language_inheritance("tsx", "typescript");
    /// ```
    pub fn register_language_inheritance(&mut self, child: impl ToString, parent: impl ToString) {
        let normalized_child = Self::normalize_lang_name(&child.to_string());
        let normalized_parent = Self::normalize_lang_name(&parent.to_string());
        self.language_inheritance
            .insert(normalized_child, normalized_parent);
    }

    /// Gets the grammar name to use for loading the actual grammar.
    /// If the language has a parent, returns the parent's name.
    fn get_grammar_name(&self, lang_name: &str) -> String {
        let normalized = Self::normalize_lang_name(lang_name);
        self.language_inheritance
            .get(&normalized)
            .cloned()
            .unwrap_or(normalized)
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
    /// For languages with inheritance, queries are loaded from the child's directory first,
    /// falling back to the parent if not found.
    pub fn get_query(&mut self, lang_name: &str, query_name: &str) -> Option<Arc<Query>> {
        let normalized_lang = Self::normalize_lang_name(lang_name);
        let grammar_name = self.get_grammar_name(lang_name);
        let language = self.get_language(&grammar_name)?;

        // Cache check
        if let Some(queries) = self.loaded_queries.get(&normalized_lang)
            && let Some(query) = queries.get(query_name)
        {
            return Some(query.clone());
        }

        let query_filename = if query_name.ends_with('s') {
            format!("{}.scm", query_name.to_lowercase())
        } else {
            format!("{}s.scm", query_name.to_lowercase())
        };

        // Generate possible name variations for directory lookup
        let mut variants = vec![
            lang_name.to_string(),
            lang_name.replace('_', "-"),
            lang_name.replace('_', "."),
            normalized_lang.replace('_', "-"),
            normalized_lang.replace('_', "."),
        ];

        // Ensure uniqueness
        variants.sort();
        variants.dedup();

        let mut paths_to_check = Vec::new();

        // Add search paths for each variant
        for variant in &variants {
            // Shared child query dir
            paths_to_check.push(
                self.base_path
                    .join("..")
                    .join("queries")
                    .join(variant)
                    .join(&query_filename),
            );
            // Child grammar query dir
            paths_to_check.push(
                self.base_path
                    .join(format!("tree-sitter-{}", variant))
                    .join("queries")
                    .join(&query_filename),
            );
        }

        // If inherited, do the same for the parent
        if let Some(parent_name) = self.language_inheritance.get(&normalized_lang) {
            let parent_variants = vec![
                parent_name.clone(),
                parent_name.replace('_', "-"),
                parent_name.replace('_', "."),
            ];
            for variant in parent_variants {
                paths_to_check.push(
                    self.base_path
                        .join("..")
                        .join("queries")
                        .join(&variant)
                        .join(&query_filename),
                );
                paths_to_check.push(
                    self.base_path
                        .join(format!("tree-sitter-{}", variant))
                        .join("queries")
                        .join(&query_filename),
                );
            }
        }

        // Find the first existing path
        let query_path = paths_to_check.into_iter().find(|p| p.exists())?;

        // Try to load it
        if let Ok(mut query_source) = std::fs::read_to_string(&query_path) {
            // Check for inheritance directive
            let mut inherited_text = String::new();
            if let Some(first_line) = query_source.lines().next() {
                if let Some(rest) = first_line.strip_prefix("; inherit:") {
                    let inherited_lang = rest.trim();
                    tracing::debug!(
                        "Query '{}' for '{}' inherits from '{}'",
                        query_name,
                        normalized_lang,
                        inherited_lang
                    );

                    // Load parent query recursively
                    if self.get_query(inherited_lang, query_name).is_some() {
                        // Convert Arc<Query> → String by reading file again from disk (simplest and safe)
                        let inherited_query_path = self
                            .base_path
                            .join(format!("tree-sitter-{}", inherited_lang))
                            .join("queries")
                            .join(format!("{}s.scm", query_name));

                        if let Ok(parent_text) = std::fs::read_to_string(&inherited_query_path) {
                            inherited_text = parent_text;
                        } else {
                            tracing::warn!(
                                "Inherited query file for '{}' not found: {:?}",
                                inherited_lang,
                                inherited_query_path
                            );
                        }
                    }
                }
            }

            // Compose inherited + current
            if !inherited_text.is_empty() {
                query_source = format!("{inherited_text}\n{query_source}");
            }

            // Compile
            let query = match Query::new(&language, &query_source) {
                Ok(q) => Arc::new(q),
                Err(e) => {
                    tracing::error!(
                        "Failed to parse query file for '{}' (query: {}): {:?}",
                        normalized_lang,
                        query_name,
                        e
                    );
                    return None;
                }
            };

            self.loaded_queries
                .entry(normalized_lang)
                .or_default()
                .insert(query_name.to_string(), query.clone());

            Some(query)
        } else {
            None
        }
    }

    /// If the language inherits from another, the parent's grammar will be loaded.
    pub fn get_language(&mut self, name: &str) -> Option<Language> {
        let grammar_name = self.get_grammar_name(name);

        // Check cache with grammar name (not the child alias)
        if let Some(lang) = self.loaded_grammars.get(&grammar_name) {
            return Some(lang.clone());
        }

        tracing::info!("Couldn't find `{name}` in loaded tree-sitter grammars, loading...");

        // Use grammar name for directory path (the parent if inherited)
        let lib_path = self.base_path.join(format!("tree-sitter-{}", grammar_name));
        let lib_filename = format!("{}.so", grammar_name);

        let lib_file = lib_path.join(lib_filename);

        unsafe {
            let library = Library::new(&lib_file).ok()?;
            let lib_symbol = format!("tree_sitter_{}\0", grammar_name);
            let language_func: Symbol<unsafe extern "C" fn() -> Language> =
                library.get(lib_symbol.as_bytes()).ok()?;

            let language = language_func();

            std::mem::forget(library);

            // Cache with grammar name (so parent is cached once for all children)
            self.loaded_grammars.insert(grammar_name, language.clone());
            Some(language)
        }
    }
}
