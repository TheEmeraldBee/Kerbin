use std::path::PathBuf;

use libloading::{Library, Symbol};
use tree_sitter::Language;

#[derive(thiserror::Error, Debug)]
pub enum GrammarLoadError {
    #[error(transparent)]
    LibLoading(#[from] libloading::Error),

    #[error("Library file not found at {path} (excludes .so/.dylib/.dll)")]
    MissingFile { path: String },
}

/// Normalizes a language name by replacing -, ., and _ with a single underscore
/// This is used for internal storage and lookups
pub fn normalize_lang_name(name: &str) -> String {
    name.replace(['-', '.'], "_")
}

/// Helps to define a Grammar using a default definition as well as aliases
///
/// Aliases are used to point to other grammars
#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum GrammarEntry {
    #[serde(rename = "def")]
    Definition(GrammarDefinition),

    #[serde(rename = "alias")]
    Alias {
        base_lang: String,
        new_name: String,
        exts: Vec<String>,
    },
}

#[derive(serde::Deserialize, Clone)]
pub struct GrammarDefinition {
    /// The custom entrypoint for the lang (defaults to tree_sitter_{name})
    pub entry: Option<String>,

    /// The custom path to the language (default to
    /// {config_path}/runtime/grammars/{name}/{name})
    /// Don't include .so/.dylib/.dll, this is automatically checked
    ///
    pub location: Option<String>,

    /// Name of the language (will be normalized internally)
    pub name: String,

    /// Valid extensions for the grammar
    #[serde(default)]
    pub exts: Vec<String>,

    /// Grammar Install Definition
    #[serde(flatten)]
    pub install: Option<GrammarInstallDefinition>,
}

impl GrammarDefinition {
    /// Gets the normalized name used for internal storage
    pub fn normalized_name(&self) -> String {
        normalize_lang_name(&self.name)
    }

    /// Locates the name for the file of the grammar, returning it's probable location
    /// This does not check for the file existing
    /// Returns all possible paths with -, _, and . variations
    pub fn get_file_paths(&self, config_path: &str) -> Vec<String> {
        if let Some(location) = &self.location {
            return vec![location.clone()];
        }

        let normalized = self.normalized_name();
        let variants = get_name_variants(&self.name);

        let mut paths = Vec::new();
        for variant in variants {
            paths.push(format!(
                "{config_path}/runtime/grammars/tree-sitter-{0}/{1}",
                variant, normalized
            ));
        }
        paths
    }

    /// Figures out the name of the entry symbol for the grammar
    pub fn get_symbol_name(&self) -> String {
        match &self.entry {
            Some(t) => t.clone(),
            None => format!("tree_sitter_{}", self.normalized_name()),
        }
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct GrammarInstallDefinition {
    /// The git-url for the grammar
    pub url: String,

    /// The .so/.dylib/.dll that is created
    /// Defaults to language name
    pub build_name: Option<String>,

    /// The Sub-directory that should be entered into the git repository
    pub sub_dir: Option<String>,
}

/// Represents a loaded grammar file
pub struct Grammar {
    /// The normalized name of the language for the grammar
    pub name: String,

    /// The loaded language file for the grammar
    pub lang: Language,

    /// The dynamic library for the grammar
    pub lib: Library,
}

/// Gets all possible variants of a name with -, _, and .
fn get_name_variants(name: &str) -> Vec<String> {
    let mut variants = vec![name.to_string()];

    if name.contains('-') {
        variants.push(name.replace('-', "_"));
        variants.push(name.replace('-', "."));
    }
    if name.contains('_') {
        variants.push(name.replace('_', "-"));
        variants.push(name.replace('_', "."));
    }
    if name.contains('.') {
        variants.push(name.replace('.', "-"));
        variants.push(name.replace('.', "_"));
    }

    variants.sort();
    variants.dedup();
    variants
}

/// Locates a .so/.dylib/.dll depending on the active OS
/// Tries all name variants with -, _, and .
pub fn find_library(base_paths: &[String]) -> Option<PathBuf> {
    for base_path in base_paths {
        for ext in get_platform_extensions() {
            let path = PathBuf::from(format!("{}.{}", base_path, ext));
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
const fn get_platform_extensions() -> &'static [&'static str] {
    &["dll"]
}

#[cfg(target_os = "macos")]
const fn get_platform_extensions() -> &'static [&'static str] {
    &["dylib"]
}

#[cfg(target_os = "linux")]
const fn get_platform_extensions() -> &'static [&'static str] {
    &["so"]
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
const fn get_platform_extensions() -> &'static [&'static str] {
    // For other Unix-like systems, try .so first
    &["so", "dylib"]
}

impl Grammar {
    /// Loads a grammar, capturing errors and searching for the correct filetype automatically
    pub fn load(name: &str, paths: &[String], symbol_name: &str) -> Result<Self, GrammarLoadError> {
        let path = match find_library(paths) {
            Some(t) => t,
            None => {
                return Err(GrammarLoadError::MissingFile {
                    path: paths.first().unwrap_or(&String::new()).to_string(),
                });
            }
        };

        unsafe {
            let lib = Library::new(&path)?;
            let func: Symbol<unsafe extern "C" fn() -> Language> =
                lib.get(symbol_name.as_bytes())?;
            let lang = func();
            Ok(Self {
                name: normalize_lang_name(name),
                lang,
                lib,
            })
        }
    }

    /// Loads a grammar from a GrammarDefinition, getting correct paths automatically
    pub fn from_def(config_path: &str, def: &GrammarDefinition) -> Result<Self, GrammarLoadError> {
        let paths = def.get_file_paths(config_path);
        let symbol = def.get_symbol_name();

        Self::load(&def.name, &paths, &symbol)
    }
}
