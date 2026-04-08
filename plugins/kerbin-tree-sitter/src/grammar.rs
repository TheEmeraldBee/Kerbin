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

/// Normalizes a language name by replacing special characters with a single underscore
pub fn normalize_lang_name(name: &str) -> String {
    name.replace(['-', '.'], "_")
}

/// Helps to define a Grammar using a default definition as well as aliases
#[derive(serde::Deserialize, Clone)]
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
    /// Override for the C entry symbol (defaults to `tree_sitter_{normalized_name}`)
    pub entry: Option<String>,

    /// Override for the library search path
    pub location: Option<String>,

    pub name: String,

    #[serde(default)]
    pub exts: Vec<String>,

    #[serde(flatten)]
    pub install: Option<GrammarInstallDefinition>,
}

impl GrammarDefinition {
    pub fn normalized_name(&self) -> String {
        normalize_lang_name(&self.name)
    }

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

    pub fn get_symbol_name(&self) -> String {
        match &self.entry {
            Some(t) => t.clone(),
            None => format!("tree_sitter_{}", self.normalized_name()),
        }
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct GrammarInstallDefinition {
    pub url: String,
    pub build_name: Option<String>,
    pub sub_dir: Option<String>,
}

pub struct Grammar {
    pub name: String,
    pub lang: Language,
    pub lib: Library,
}

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

/// Locates a shared library depending on the active OS
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
pub const fn get_platform_extensions() -> &'static [&'static str] {
    &["dll"]
}

#[cfg(target_os = "macos")]
pub const fn get_platform_extensions() -> &'static [&'static str] {
    &["dylib"]
}

#[cfg(target_os = "linux")]
pub const fn get_platform_extensions() -> &'static [&'static str] {
    &["so"]
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub const fn get_platform_extensions() -> &'static [&'static str] {
    // For other Unix-like systems, try .so first
    &["so", "dylib"]
}

impl Grammar {
    /// Loads a grammar capturing errors and searching for the correct filetype automatically
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

    /// Loads a grammar from a GrammarDefinition getting correct paths automatically
    pub fn from_def(config_path: &str, def: &GrammarDefinition) -> Result<Self, GrammarLoadError> {
        let paths = def.get_file_paths(config_path);
        let symbol = def.get_symbol_name();

        Self::load(&def.name, &paths, &symbol)
    }
}
