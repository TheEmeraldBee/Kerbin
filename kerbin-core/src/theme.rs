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
}
