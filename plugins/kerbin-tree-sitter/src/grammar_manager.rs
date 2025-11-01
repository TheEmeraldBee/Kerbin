use std::collections::HashMap;

use kerbin_core::{kerbin_macros::State, *};

use crate::grammar::{Grammar, GrammarDefinition};

#[derive(thiserror::Error, Debug)]
pub enum GrammarManagerError {
    #[error(transparent)]
    LibLoading(#[from] libloading::Error),

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
    pub loaded_grammars: HashMap<String, Grammar>,
}

impl GrammarManager {
    /// Registers all handlers for each extension in the map
    pub async fn register_extension_handlers(&self, state: &mut State) {
        for ext in self.ext_map.keys() {
            // TODO: Finish implementation for this
            state.on_hook(hooks::UpdateFiletype::new(ext));
        }
    }

    /// Creates the Manager by loading in a list of definitions
    pub fn from_definitions(definitions: Vec<GrammarDefinition>) -> Self {
        let mut ret = Self::default();

        for definition in definitions {
            for ext in &definition.exts {
                ret.ext_map
                    .insert(definition.name.clone(), ext.to_lowercase());
            }

            ret.lang_map.insert(definition.name.clone(), definition);
        }

        ret
    }

    /// Attempts to return a grammar, attempting to load it if it isn't already
    pub fn get_grammar(
        &mut self,
        config_path: &str,
        ext: &str,
    ) -> Result<Option<&Grammar>, GrammarManagerError> {
        let Some(lang) = self.ext_map.get(&ext.to_lowercase()) else {
            return Ok(None);
        };

        if self.loaded_grammars.contains_key(lang) {
            return Ok(Some(
                self.loaded_grammars
                    .get(lang)
                    .expect("Grammar just checked for existing"),
            ));
        }

        // Not found, load it here
        let def = self
            .lang_map
            .get(lang)
            .ok_or(GrammarManagerError::MissingDefinition { lang: lang.clone() })?;

        let grammar = Grammar::from_def(config_path, def)?;

        self.loaded_grammars.insert(lang.clone(), grammar);
        Ok(Some(
            self.loaded_grammars
                .get(lang)
                .expect("Just inserted language"),
        ))
    }
}
