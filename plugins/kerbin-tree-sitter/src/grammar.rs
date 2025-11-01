use std::path::PathBuf;

use libloading::{Library, Symbol};
use tree_sitter::Language;

#[derive(serde::Deserialize)]
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
}

pub struct Grammar {
    pub lang: Language,
    pub lib: Library,
}

impl Grammar {
    pub fn load(path: impl Into<PathBuf>, symbol_name: &str) -> Result<Self, libloading::Error> {
        let path = path.into();

        unsafe {
            let lib = Library::new(&path)?;
            let func: Symbol<unsafe extern "C" fn() -> Language> =
                lib.get(symbol_name.as_bytes())?;
            let lang = func();
            Ok(Self { lang, lib })
        }
    }

    pub fn from_def(config_path: &str, def: &GrammarDefinition) -> Result<Self, libloading::Error> {
        let path = match &def.location {
            Some(t) => t,
            None => &format!("{config_path}/runtime/grammars/{0}/{0}", def.name),
        };

        let symbol = match &def.entry {
            Some(t) => t,
            None => &def.name,
        };

        Self::load(path, symbol)
    }
}
