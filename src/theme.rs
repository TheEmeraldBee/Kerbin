use std::collections::HashMap;

use ascii_forge::window::{Attributes, Color, ContentStyle};
use rune::{Any, alloc::clone::TryClone};

/// The style that can be put on content.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Any, TryClone)]
#[rune(constructor)]
pub struct EditorStyle {
    /// The foreground color.
    #[rune(get, set, copy)]
    pub fg: Option<(u8, u8, u8)>,
    /// The background color.
    #[rune(get, set, copy)]
    pub bg: Option<(u8, u8, u8)>,
}

impl EditorStyle {
    pub fn to_content_style(&self) -> ContentStyle {
        ContentStyle {
            foreground_color: self.fg.map(|(r, g, b)| Color::Rgb { r, g, b }),
            background_color: self.bg.map(|(r, g, b)| Color::Rgb { r, g, b }),
            underline_color: None,
            attributes: Attributes::none(),
        }
    }
}

#[derive(Any, Debug, Clone, Default, PartialEq)]
pub struct Theme {
    pub theme_map: HashMap<String, EditorStyle>,
}

impl Theme {
    /// Registers a theme with a given key to a style
    /// Used for theming backgrounds and stuff
    #[rune::function(keep)]
    pub fn register(&mut self, name: String, style: EditorStyle) {
        self.theme_map.insert(name, style);
    }

    /// Returns an optional theme value if one is set.
    /// Used for theming text to make it pretty in the user's editor
    #[rune::function(keep)]
    pub fn get(&self, name: &str) -> Option<EditorStyle> {
        self.theme_map.get(name).cloned()
    }
}
