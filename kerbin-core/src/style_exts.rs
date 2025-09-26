use ascii_forge::window::ContentStyle;

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
