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

    /// Maps a grammar name to a map of its loaded queries by name (e.g., "highlight").
    loaded_queries: HashMap<String, HashMap<String, Arc<Query>>>,

    /// Maps a grammar name (e.g., "rust") to its loaded Language object.
    loaded_grammars: HashMap<String, Language>,

    /// Maps a file extension (e.g., "rs") to a grammar name ("rust").
    pub extension_map: HashMap<String, String>,
}

impl GrammarManager {
    pub fn new(base_path: String) -> Self {
        Self {
            base_path: PathBuf::from(base_path).join("runtime/grammars"),
            loaded_grammars: HashMap::new(),
            loaded_queries: HashMap::new(),
            extension_map: HashMap::new(),
        }
    }

    /// Cleans up the grammar directory after a successful build, keeping only the
    /// compiled library and query files.
    fn cleanup_grammar_directory(dir: &Path, lang_name: &str) -> io::Result<()> {
        let essential_files: Vec<String> = vec![
            format!("{}.so", lang_name.replace("-", "_")),
            format!("{}.dll", lang_name.replace("-", "_")),
            format!("{}.dylib", lang_name.replace("-", "_")),
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
    /// Installs to the config's runtime/grammars path
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

        let final_grammar_dir = config
            .base_path
            .join(format!("tree-sitter-{}", config.lang_name));

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

            let lang_name = &config.lang_name;

            let initial_filename_so = format!(
                "{}.so",
                config
                    .special_rename
                    .clone()
                    .unwrap_or(lang_name.replace("-", "_"))
            );
            let initial_filename_dll = format!(
                "{}.dll",
                config
                    .special_rename
                    .clone()
                    .unwrap_or(lang_name.replace("-", "_"))
            );
            let initial_filename_dylib = format!(
                "{}.dylib",
                config
                    .special_rename
                    .clone()
                    .unwrap_or(lang_name.replace("-", "_"))
            );

            let lib_filename_so = format!("{}.so", lang_name.replace("-", "_"));
            let lib_filename_dll = format!("{}.dll", lang_name.replace("-", "_"));
            let lib_filename_dylib = format!("{}.dylib", lang_name.replace("-", "_"));

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
                    lang_name,
                    build_dir.join("queries"),
                    build_dir.join("src/queries")
                );
            }

            Self::cleanup_grammar_directory(&final_grammar_dir, lang_name)
                .map_err(|e| format!("Failed to cleanup build dir {e}"))?;

            Ok(config.lang_name)
        })();

        // Always clean up the build directory after installation (success or failure)
        if repo_clone_dir.exists() {
            tracing::debug!("Cleaning up build directory: {:?}", repo_clone_dir);
            fs::remove_dir_all(&repo_clone_dir).ok();
        }

        result
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

        // Define all the query path components separately for clarity
        let query_dir_components: [String; 3] = [
            "queries".to_string(),
            lang_name.to_string(),
            query_filename.clone(),
        ];

        let path1 = self
            .base_path
            .join(format!("tree-sitter-{}", lang_name))
            .join("queries")
            .join(query_filename);

        let mut path2 = self.base_path.join(".."); // Go up one directory
        for component in query_dir_components {
            // Add "queries", lang_name, query_filename
            path2.push(component);
        }

        let paths_to_check = vec![path2, path1]; // Go by ../queries first

        let mut found_path: Option<PathBuf> = None;

        for path in paths_to_check {
            if path.exists() {
                found_path = Some(path);
                break; // Stop checking once found
            }
        }

        let query_path = found_path?;

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

        tracing::info!("Couldn't find `{name}` in loaded tree-sitter grammars, loading...");

        let lib_path = self.base_path.join(format!("tree-sitter-{name}"));
        let lib_filename = format!("{}.so", name.replace("-", "_"));

        let lib_file = lib_path.join(lib_filename);

        unsafe {
            let library = Library::new(&lib_file).ok()?;
            let lib_symbol = format!("tree_sitter_{}\0", name.replace("-", "_"));
            let language_func: Symbol<unsafe extern "C" fn() -> Language> =
                library.get(lib_symbol.as_bytes()).ok()?;

            let language = language_func();

            std::mem::forget(library);

            self.loaded_grammars
                .insert(name.to_string(), language.clone());
            Some(language)
        }
    }
}
