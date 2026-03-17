use std::collections::HashMap;

use kerbin_macros::State;
use kerbin_state_machine::storage::*;
use ratatui::style::{Color, Modifier, Style};

/// Parse a color name (named, or `#RRGGBB` hex) into a `Color`.
pub fn color_from_str(value: &str) -> Option<Color> {
    let v = value.to_lowercase();
    match v.as_str() {
        "black" => Some(Color::Black),
        "darkgrey" | "dark_grey" => Some(Color::DarkGray),
        "red" => Some(Color::Red),
        "darkred" | "dark_red" => Some(Color::Rgb(128, 0, 0)),
        "green" => Some(Color::Green),
        "darkgreen" | "dark_green" => Some(Color::Rgb(0, 128, 0)),
        "yellow" => Some(Color::Yellow),
        "darkyellow" | "dark_yellow" => Some(Color::Rgb(128, 128, 0)),
        "blue" => Some(Color::Blue),
        "darkblue" | "dark_blue" => Some(Color::Rgb(0, 0, 128)),
        "magenta" => Some(Color::Magenta),
        "darkmagenta" | "dark_magenta" => Some(Color::Rgb(128, 0, 128)),
        "cyan" => Some(Color::Cyan),
        "darkcyan" | "dark_cyan" => Some(Color::Rgb(0, 128, 128)),
        "white" => Some(Color::White),
        "grey" | "gray" => Some(Color::Gray),
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16).ok()?;
            let g = u8::from_str_radix(&s[3..5], 16).ok()?;
            let b = u8::from_str_radix(&s[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

/// Parse an attribute name into a `Modifier`.
pub fn attr_from_str(value: &str) -> Option<Modifier> {
    match value.to_lowercase().as_str() {
        "bold" => Some(Modifier::BOLD),
        "dim" => Some(Modifier::DIM),
        "italic" => Some(Modifier::ITALIC),
        "underlined" => Some(Modifier::UNDERLINED),
        "slowblink" => Some(Modifier::SLOW_BLINK),
        "rapidblink" => Some(Modifier::RAPID_BLINK),
        "reversed" => Some(Modifier::REVERSED),
        "hidden" => Some(Modifier::HIDDEN),
        "crossedout" => Some(Modifier::CROSSED_OUT),
        _ => None,
    }
}

/// Resolve a color string: first try palette lookup, then direct parse.
pub fn resolve_color(name: &str, palette: &HashMap<String, Color>) -> Option<Color> {
    palette.get(name).copied().or_else(|| color_from_str(name))
}

/// Build a `Style` from optional fg/bg/underline color names and attribute names.
pub fn build_style(
    fg: Option<&str>,
    bg: Option<&str>,
    underline: Option<&str>,
    attrs: &[String],
    palette: &HashMap<String, Color>,
) -> Style {
    let mut style = Style::default();
    let mut modifiers = Modifier::empty();
    if let Some(c) = fg.and_then(|s| resolve_color(s, palette)) {
        style = style.fg(c);
    }
    if let Some(c) = bg.and_then(|s| resolve_color(s, palette)) {
        style = style.bg(c);
    }
    if let Some(c) = underline.and_then(|s| resolve_color(s, palette)) {
        style = style.underline_color(c);
    }
    for a in attrs {
        if let Some(attr) = attr_from_str(a) {
            modifiers |= attr;
        }
    }
    style.add_modifier(modifiers)
}

/// A wrapper over the internal storage for themes
#[derive(Default, State)]
pub struct Theme {
    /// The internal hash map storing theme names (strings) to their corresponding `Style`
    map: HashMap<String, Style>,
}

impl Theme {
    /// Registers a theme, associating a `Style` with a given name
    pub fn register(&mut self, name: String, style: Style) {
        self.map.insert(name, style);
    }

    /// Retrieves a `Style` from the system by its name
    pub fn get(&self, name: &str) -> Option<Style> {
        self.map.get(name).copied()
    }

    /// Retrieves a `Style` based on an iterator of names, falling back to a default style
    pub fn get_fallback_default(
        &self,
        names: impl IntoIterator<Item = impl ToString>,
    ) -> Style {
        for name in names.into_iter().map(|x| x.to_string()) {
            if let Some(theme) = self.get(&name) {
                return theme;
            }
        }
        Style::default()
    }
}
