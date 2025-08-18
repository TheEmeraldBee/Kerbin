use ascii_forge::prelude::*;
use kerbin_core::{InputEvent, State};
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::sync::Arc;

#[derive(Debug)]
pub enum ThemeError {
    InvalidColor(String),
    UnknownAttribute(String),
    UnresolvedPaletteReference(String),
    CyclicPaletteReference(String),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeError::InvalidColor(c) => {
                write!(f, "invalid color value: '{}'", c)
            }
            ThemeError::UnknownAttribute(a) => {
                write!(f, "unknown attribute: '{}'", a)
            }
            ThemeError::UnresolvedPaletteReference(r) => {
                write!(f, "unresolved palette reference: '{}'", r)
            }
            ThemeError::CyclicPaletteReference(r) => {
                write!(f, "cyclic palette reference involving: '{}'", r)
            }
        }
    }
}

impl Error for ThemeError {}

fn color_from_str(value: &str) -> Result<Color, ThemeError> {
    let value = value.to_lowercase();
    match value.as_str() {
        "black" => Ok(Color::Black),
        "darkgrey" => Ok(Color::DarkGrey),
        "red" => Ok(Color::Red),
        "darkred" => Ok(Color::DarkRed),
        "green" => Ok(Color::Green),
        "darkgreen" => Ok(Color::DarkGreen),
        "yellow" => Ok(Color::Yellow),
        "darkyellow" => Ok(Color::DarkYellow),
        "blue" => Ok(Color::Blue),
        "darkblue" => Ok(Color::DarkBlue),
        "magenta" => Ok(Color::Magenta),
        "darkmagenta" => Ok(Color::DarkMagenta),
        "cyan" => Ok(Color::Cyan),
        "darkcyan" => Ok(Color::DarkCyan),
        "white" => Ok(Color::White),
        "grey" => Ok(Color::Grey),
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16)
                .map_err(|_| ThemeError::InvalidColor(s.to_string()))?;
            let g = u8::from_str_radix(&s[3..5], 16)
                .map_err(|_| ThemeError::InvalidColor(s.to_string()))?;
            let b = u8::from_str_radix(&s[5..7], 16)
                .map_err(|_| ThemeError::InvalidColor(s.to_string()))?;
            Ok(Color::Rgb { r, g, b })
        }
        _ => Err(ThemeError::InvalidColor(value.to_string())),
    }
}

#[derive(Debug, Default)]
pub struct UnresolvedStyle {
    fg: Option<String>,
    bg: Option<String>,
    underline: Option<String>,
    attrs: Vec<String>,
}

impl<'de> Deserialize<'de> for UnresolvedStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UnresolvedStyleVisitor;

        impl<'de> Visitor<'de> for UnresolvedStyleVisitor {
            type Value = UnresolvedStyle;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string for foreground color, or a style table")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(UnresolvedStyle {
                    fg: Some(value.to_string()),
                    ..Default::default()
                })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut style = UnresolvedStyle::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "fg" => style.fg = Some(map.next_value()?),
                        "bg" => style.bg = Some(map.next_value()?),
                        "underline" => style.underline = Some(map.next_value()?),
                        "attrs" => style.attrs = map.next_value()?,
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                Ok(style)
            }
        }

        deserializer.deserialize_any(UnresolvedStyleVisitor)
    }
}

impl UnresolvedStyle {
    pub fn resolve(self, palette: &HashMap<String, Color>) -> Result<ContentStyle, ThemeError> {
        let mut style = ContentStyle::new();
        let mut attributes = Attributes::none();

        let resolve_color = |name: &str| {
            if let Some(color) = palette.get(name) {
                Ok(*color)
            } else {
                color_from_str(name)
            }
        };

        if let Some(fg) = self.fg {
            style.foreground_color = Some(resolve_color(&fg)?);
        }
        if let Some(bg) = self.bg {
            style.background_color = Some(resolve_color(&bg)?);
        }
        if let Some(underline) = self.underline {
            style.underline_color = Some(resolve_color(&underline)?);
        }

        for attr_str in self.attrs {
            let attr = match attr_str.to_lowercase().as_str() {
                "bold" => Attribute::Bold,
                "dim" => Attribute::Dim,
                "italic" => Attribute::Italic,
                "underlined" => Attribute::Underlined,
                "slowblink" => Attribute::SlowBlink,
                "rapidblink" => Attribute::RapidBlink,
                "reversed" => Attribute::Reverse,
                "hidden" => Attribute::Hidden,
                "crossedout" => Attribute::CrossedOut,
                _ => return Err(ThemeError::UnknownAttribute(attr_str)),
            };
            attributes.set(attr);
        }
        style.attributes = attributes;

        Ok(style)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl<'de> Deserialize<'de> for KeyBind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KeyBindVisitor;

        impl<'de> Visitor<'de> for KeyBindVisitor {
            type Value = KeyBind;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string like 'ctrl-s' or 'up'")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let mut parts: Vec<&str> = value.split('-').collect();
                if parts.is_empty() {
                    return Err(E::custom("Input cannot be empty"));
                }

                let key_str = parts.pop().unwrap().to_lowercase();
                let mut modifiers = KeyModifiers::empty();

                for part in parts {
                    match part.to_lowercase().as_str() {
                        "ctrl" | "control" => modifiers.insert(KeyModifiers::CONTROL),
                        "alt" => modifiers.insert(KeyModifiers::ALT),
                        "shift" => modifiers.insert(KeyModifiers::SHIFT),
                        "super" => modifiers.insert(KeyModifiers::SUPER),
                        "hyper" => modifiers.insert(KeyModifiers::HYPER),
                        "meta" => modifiers.insert(KeyModifiers::META),
                        _ => return Err(E::custom(format!("Unknown modifier: {}", part))),
                    }
                }

                let code = match key_str.as_str() {
                    "backspace" => KeyCode::Backspace,
                    "enter" => KeyCode::Enter,
                    "left" => KeyCode::Left,
                    "right" => KeyCode::Right,
                    "up" => KeyCode::Up,
                    "down" => KeyCode::Down,
                    "home" => KeyCode::Home,
                    "end" => KeyCode::End,
                    "pageup" => KeyCode::PageUp,
                    "pagedown" => KeyCode::PageDown,
                    "tab" => KeyCode::Tab,
                    "backtab" => KeyCode::BackTab,
                    "delete" => KeyCode::Delete,
                    "insert" => KeyCode::Insert,
                    "esc" => KeyCode::Esc,
                    "space" => KeyCode::Char(' '),
                    s if s.starts_with('f') && s.len() > 1 => {
                        let num = s[1..]
                            .parse::<u8>()
                            .map_err(|_| E::custom(format!("Invalid F-key: {}", s)))?;
                        KeyCode::F(num)
                    }
                    s if s.chars().count() == 1 => KeyCode::Char(s.chars().next().unwrap()),
                    _ => return Err(E::custom(format!("Unknown key: {}", key_str))),
                };

                Ok(KeyBind { code, modifiers })
            }
        }

        deserializer.deserialize_str(KeyBindVisitor)
    }
}

#[derive(Deserialize, Debug, Default)]
pub struct Input {
    pub modes: Vec<char>,
    pub keys: Vec<KeyBind>,
    pub commands: Vec<String>,

    #[serde(default)]
    pub desc: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(rename = "keybind")]
    keybindings: Vec<Input>,
    palette: HashMap<String, String>,
    theme: HashMap<String, UnresolvedStyle>,
}

impl Config {
    pub fn load(path: Option<String>) -> Result<Self, toml::de::Error> {
        let file_content = fs::read_to_string(path.unwrap_or("config.toml".to_string()))
            .expect("Could not read config file");
        toml::from_str(&file_content)
    }

    fn resolve_palette(&self) -> Result<HashMap<String, Color>, ThemeError> {
        let mut resolved = HashMap::new();
        let mut unresolved = self.palette.clone();
        let mut last_unresolved_count = unresolved.len() + 1;

        while !unresolved.is_empty() && unresolved.len() < last_unresolved_count {
            last_unresolved_count = unresolved.len();
            unresolved.retain(|name, value| {
                if let Ok(color) = color_from_str(value) {
                    resolved.insert(name.clone(), color);
                    false
                } else if let Some(color) = resolved.get(value) {
                    resolved.insert(name.clone(), *color);
                    false
                } else {
                    true
                }
            });
        }

        if !unresolved.is_empty() {
            let key = unresolved.keys().next().unwrap();
            if self.palette.contains_key(unresolved.get(key).unwrap()) {
                return Err(ThemeError::CyclicPaletteReference(key.clone()));
            } else {
                return Err(ThemeError::UnresolvedPaletteReference(
                    unresolved.get(key).unwrap().clone(),
                ));
            }
        }

        Ok(resolved)
    }

    pub fn apply(self, state: Arc<State>) {
        let mut inputs = state.input_config.write().unwrap();

        let palette = match self.resolve_palette() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error resolving color palette: {}", e);
                return;
            }
        };

        for input in self.keybindings {
            inputs.register_input(kerbin_core::Input {
                valid_modes: input.modes,
                key_sequence: input.keys.iter().map(|x| (x.modifiers, x.code)).collect(),
                event: InputEvent::Commands(input.commands),
                desc: input.desc,
            });
        }

        let mut theme = state.theme.write().unwrap();
        for (name, unresolved_style) in self.theme.into_iter() {
            match unresolved_style.resolve(&palette) {
                Ok(style) => theme.register(name, style),
                Err(e) => eprintln!("Error resolving theme item '{}': {}", name, e),
            }
        }
    }
}
