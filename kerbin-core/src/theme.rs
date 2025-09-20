use std::collections::HashMap;

use ascii_forge::window::ContentStyle;
use kerbin_macros::State;
use kerbin_state_machine::storage::*;

/// A wrapper over the internal storage for themes.
///
/// This struct allows for quickly retrieving and managing `ContentStyle`
/// instances throughout the editor, enabling consistent UI theming.
#[derive(Default, State)]
pub struct Theme {
    /// The internal hash map storing theme names (strings) to their corresponding `ContentStyle`.
    map: HashMap<String, ContentStyle>,
}

impl Theme {
    /// Registers a theme, associating a `ContentStyle` with a given name.
    ///
    /// If a theme with the same `name` already exists, its `ContentStyle` will be
    /// replaced by the new one provided.
    ///
    /// # Arguments
    ///
    /// * `name`: The `String` identifier for the theme (e.g., "statusline_bg", "keyword").
    /// * `style`: The `ContentStyle` to associate with the given name.
    pub fn register(&mut self, name: String, style: ContentStyle) {
        self.map.insert(name, style);
    }

    /// Retrieves a `ContentStyle` from the system by its name.
    ///
    /// This method provides direct access to a registered style. For more robust
    /// theme retrieval that includes fallback behavior, consider `get_fallback_default`.
    ///
    /// # Arguments
    ///
    /// * `name`: The string slice identifier of the theme to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option<ContentStyle>`: `Some(ContentStyle)` if the theme exists, otherwise `None`.
    pub fn get(&self, name: &str) -> Option<ContentStyle> {
        self.map.get(name).copied()
    }

    /// Retrieves a `ContentStyle` based on an iterator of names, falling back to a default style.
    ///
    /// This method iterates through the provided names in order, attempting to retrieve
    /// a registered theme. The first successful retrieval determines the returned style.
    /// If no style is found for any of the provided names, `ContentStyle::default()` is returned.
    /// This is particularly useful for applying styles with a priority order.
    ///
    /// # Arguments
    ///
    /// * `names`: An `IntoIterator` yielding items that can be converted to `String`.
    ///            These names are tried in the order they are provided.
    ///
    /// # Returns
    ///
    /// The `ContentStyle` associated with the first found name, or `ContentStyle::default()`
    /// if none of the provided names match a registered theme.
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

/// An extension trait for `ContentStyle` to provide utility methods.
pub trait ContentStyleExt {
    /// Combines two `ContentStyle` instances, giving priority to the `other` style.
    ///
    /// This function is used extensively with core rendering to layer styles on top
    /// of one another. If a field (like foreground color) is set in `other`, it
    /// overrides the value from `self`. Otherwise, the value from `self` is used.
    /// Attributes are combined using a bitwise OR.
    ///
    /// # Arguments
    ///
    /// * `other`: The `ContentStyle` to combine with, which takes priority.
    ///
    /// # Returns
    ///
    /// A new `ContentStyle` resulting from the combination.
    fn combined_with(&self, other: &ContentStyle) -> ContentStyle;
}

impl ContentStyleExt for ContentStyle {
    fn combined_with(&self, other: &ContentStyle) -> ContentStyle {
        ContentStyle {
            foreground_color: other.foreground_color.or(self.foreground_color),
            background_color: other.background_color.or(self.background_color),
            underline_color: other.underline_color.or(self.underline_color),
            attributes: self.attributes | other.attributes,
        }
    }
}
