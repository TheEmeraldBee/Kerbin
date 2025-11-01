use std::{collections::HashMap, path::PathBuf, sync::Arc};

use libloading::{Library, Symbol};
use tree_sitter::{Language, Query};

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

#[derive(serde::Deserialize, Clone)]
pub struct GrammarInstallDefinition {
    /// The git-url for the grammar
    pub url: String,

    /// The .so/.dylib/.dll that is created
    pub build_name: String,

    /// The Sub-directory that should be entered into the git repository
    pub sub_dir: Option<String>,
}

/// Represents a loaded grammar file
pub struct Grammar {
    pub name: String,

    pub lang: Language,
    pub lib: Library,

    pub queries: HashMap<String, Arc<Query>>,
}

fn find_library(base_path: &str) -> Option<PathBuf> {
    let extensions = get_platform_extensions();

    for ext in extensions {
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
fn get_platform_extensions() -> &'static [&'static str] {
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
                queries: HashMap::default(),
            })
        }
    }

    /// Loads a grammar from a GrammarDefinition, getting correct paths automatically
    pub fn from_def(config_path: &str, def: &GrammarDefinition) -> Result<Self, GrammarLoadError> {
        let path = match &def.location {
            Some(t) => t,
            None => &format!(
                "{config_path}/runtime/grammars/tree-sitter-{0}/{0}",
                def.name
            ),
        };

        let symbol = match &def.entry {
            Some(t) => t,
            None => &format!("tree_sitter_{}", def.name),
        };

        Self::load(&def.name, path, symbol)
    }
}
