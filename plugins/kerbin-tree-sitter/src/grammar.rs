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

    /// Name of the language
    pub name: String,

    /// Valid extensions for the grammar
    #[serde(default)]
    pub exts: Vec<String>,

    /// Grammar Install Definition
    #[serde(flatten)]
    pub install: Option<GrammarInstallDefinition>,
}

impl GrammarDefinition {
    /// Locates the name for the file of the grammar, returning it's probable location
    /// This does not check for the file existing
    pub fn get_file_path(&self, config_path: &str) -> String {
        match &self.location {
            Some(t) => t.clone(),
            None => format!(
                "{config_path}/runtime/grammars/tree-sitter-{0}/{0}",
                self.name
            ),
        }
    }

    /// Figures out the name of the entry symbol for the grammar
    pub fn get_symbol_name(&self) -> String {
        match &self.entry {
            Some(t) => t.clone(),
            None => format!("tree_sitter_{}", self.name),
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
    /// The name of the language for the grammar
    pub name: String,

    /// The loaded language file for the grammar
    pub lang: Language,

    /// The dynamic library for the grammar
    pub lib: Library,
}

/// Locates a .so/.dylib/.dll depending on the active OS
/// Defaults to [".so", ".dylib"] for unknown languages
pub fn find_library(base_path: &str) -> Option<PathBuf> {
    for ext in get_platform_extensions() {
        let path = PathBuf::from(format!("{}.{}", base_path, ext));
        if path.exists() {
            return Some(path);
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
    pub fn load(name: &str, path: &str, symbol_name: &str) -> Result<Self, GrammarLoadError> {
        let path = match find_library(path) {
            Some(t) => t,
            None => {
                return Err(GrammarLoadError::MissingFile {
                    path: path.to_string(),
                });
            }
        };

        unsafe {
            let lib = Library::new(&path)?;
            let func: Symbol<unsafe extern "C" fn() -> Language> =
                lib.get(symbol_name.as_bytes())?;
            let lang = func();
            Ok(Self {
                name: name.to_string(),
                lang,
                lib,
            })
        }
    }

    /// Loads a grammar from a GrammarDefinition, getting correct paths automatically
    pub fn from_def(config_path: &str, def: &GrammarDefinition) -> Result<Self, GrammarLoadError> {
        let path = &def.get_file_path(config_path);
        let symbol = &def.get_symbol_name();

        Self::load(&def.name, path, symbol)
    }
}
