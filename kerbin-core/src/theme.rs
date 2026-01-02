use std::collections::HashMap;

use ascii_forge::window::ContentStyle;
use kerbin_macros::State;
use kerbin_state_machine::storage::*;

/// A wrapper over the internal storage for themes
#[derive(Default, State)]
pub struct Theme {
    /// The internal hash map storing theme names (strings) to their corresponding `ContentStyle`
    map: HashMap<String, ContentStyle>,
}

impl Theme {
    /// Registers a theme, associating a `ContentStyle` with a given name
    pub fn register(&mut self, name: String, style: ContentStyle) {
        self.map.insert(name, style);
    }

    /// Retrieves a `ContentStyle` from the system by its name
    pub fn get(&self, name: &str) -> Option<ContentStyle> {
        self.map.get(name).copied()
    }

    /// Retrieves a `ContentStyle` based on an iterator of names, falling back to a default style
    pub fn get_fallback_default(
        &self,
        names: impl IntoIterator<Item = impl ToString>,
    ) -> ContentStyle {
        for name in names.into_iter().map(|x| x.to_string()) {
            if let Some(theme) = self.get(&name) {
                return theme;
            }
        }
        ContentStyle::default()
    }
}
