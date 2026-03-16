use std::collections::HashMap;

use ascii_forge::{prelude::{Attribute, Attributes, Color}, window::ContentStyle};
use kerbin_macros::State;
use kerbin_state_machine::storage::*;

/// Parse a color name (named, or `#RRGGBB` hex) into a `Color`.
pub fn color_from_str(value: &str) -> Option<Color> {
    let v = value.to_lowercase();
    match v.as_str() {
        "black" => Some(Color::Black),
        "darkgrey" | "dark_grey" => Some(Color::DarkGrey),
        "red" => Some(Color::Red),
        "darkred" | "dark_red" => Some(Color::DarkRed),
        "green" => Some(Color::Green),
        "darkgreen" | "dark_green" => Some(Color::DarkGreen),
        "yellow" => Some(Color::Yellow),
        "darkyellow" | "dark_yellow" => Some(Color::DarkYellow),
        "blue" => Some(Color::Blue),
        "darkblue" | "dark_blue" => Some(Color::DarkBlue),
        "magenta" => Some(Color::Magenta),
        "darkmagenta" | "dark_magenta" => Some(Color::DarkMagenta),
        "cyan" => Some(Color::Cyan),
        "darkcyan" | "dark_cyan" => Some(Color::DarkCyan),
        "white" => Some(Color::White),
        "grey" => Some(Color::Grey),
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16).ok()?;
            let g = u8::from_str_radix(&s[3..5], 16).ok()?;
            let b = u8::from_str_radix(&s[5..7], 16).ok()?;
            Some(Color::Rgb { r, g, b })
        }
        _ => None,
    }
}

/// Parse an attribute name into an `Attribute`.
pub fn attr_from_str(value: &str) -> Option<Attribute> {
    match value.to_lowercase().as_str() {
        "bold" => Some(Attribute::Bold),
        "dim" => Some(Attribute::Dim),
        "italic" => Some(Attribute::Italic),
        "underlined" => Some(Attribute::Underlined),
        "slowblink" => Some(Attribute::SlowBlink),
        "rapidblink" => Some(Attribute::RapidBlink),
        "reversed" => Some(Attribute::Reverse),
        "hidden" => Some(Attribute::Hidden),
        "crossedout" => Some(Attribute::CrossedOut),
        _ => None,
    }
}

/// Resolve a color string: first try palette lookup, then direct parse.
pub fn resolve_color(name: &str, palette: &HashMap<String, Color>) -> Option<Color> {
    palette.get(name).copied().or_else(|| color_from_str(name))
}

/// Build a `ContentStyle` from optional fg/bg/underline color names and attribute names.
pub fn build_content_style(
    fg: Option<&str>,
    bg: Option<&str>,
    underline: Option<&str>,
    attrs: &[String],
    palette: &HashMap<String, Color>,
) -> ContentStyle {
    let mut style = ContentStyle::default();
    let mut attributes = Attributes::none();
    if let Some(c) = fg.and_then(|s| resolve_color(s, palette)) {
        style.foreground_color = Some(c);
    }
    if let Some(c) = bg.and_then(|s| resolve_color(s, palette)) {
        style.background_color = Some(c);
    }
    if let Some(c) = underline.and_then(|s| resolve_color(s, palette)) {
        style.underline_color = Some(c);
    }
    for a in attrs {
        if let Some(attr) = attr_from_str(a) {
            attributes = attributes.with(attr);
        }
    }
    style.attributes = attributes;
    style
}

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
