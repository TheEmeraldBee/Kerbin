use std::collections::HashMap;

use ascii_forge::window::ContentStyle;

#[derive(Default)]
pub struct Theme {
    map: HashMap<String, ContentStyle>,
}

impl Theme {
    pub fn register(&mut self, name: String, style: ContentStyle) {
        self.map.insert(name, style);
    }

    pub fn get(&self, name: &str) -> Option<ContentStyle> {
        self.map.get(name).copied()
    }

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
