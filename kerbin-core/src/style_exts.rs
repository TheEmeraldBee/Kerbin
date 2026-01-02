use ascii_forge::window::ContentStyle;

/// An extension trait for `ContentStyle` to provide utility methods
pub trait ContentStyleExt {
    /// Combines two `ContentStyle` instances, giving priority to the `other` style
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
