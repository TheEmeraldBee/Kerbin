use ascii_forge::prelude::*;
use kerbin_core::{CommandPrefix, CommandPrefixRegistry, InputConfig, Theme};
use kerbin_core::{InputEvent, PluginConfig};
use kerbin_state_machine::State;
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

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
            attributes = attributes.with(attr);
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

    #[serde(default)]
    pub invalid_modes: Vec<char>,

    pub keys: Vec<KeyBind>,
    pub commands: Vec<String>,

    #[serde(default)]
    pub desc: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct Prefix {
    pub modes: Vec<char>,
    pub prefix: String,

    #[serde(default)]
    pub include: bool,

    #[serde(default)]
    pub list: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportEntry {
    pub paths: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Config {
    keybindings: Vec<Input>,
    prefixes: Vec<Prefix>,

    palette: HashMap<String, String>,
    theme: HashMap<String, UnresolvedStyle>,
    plugin_config: HashMap<String, Value>,
}

impl Config {
    pub fn load(path: impl ToString) -> Result<Self, Box<dyn Error>> {
        let path = PathBuf::from(path.to_string());
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Result<Self, Box<dyn Error>> {
        let file_content = fs::read_to_string(path)?;
        let toml_table: Table = toml::from_str(&file_content)?; // Use toml::Table to preserve order

        let mut final_config = Config::default();
        let base_dir = path.parent().unwrap_or_else(|| Path::new(""));

        // Iterate through the top-level keys in the order they appear in the file
        for (key, value) in toml_table.into_iter() {
            match key.as_str() {
                "import" => {
                    if let Value::Array(imports_array) = value {
                        for import_val in imports_array {
                            if let Ok(import_entry) = import_val.try_into::<ImportEntry>() {
                                for path in import_entry.paths {
                                    let imported_path = base_dir.join(&path);
                                    let imported_config = Config::load_from_path(&imported_path)?;
                                    final_config.merge(imported_config);
                                }
                            } else {
                                return Err(format!("Invalid import entry in {:?}", path).into());
                            }
                        }
                    } else {
                        return Err(format!(
                            "`import` section in {:?} must be an array of tables",
                            path
                        )
                        .into());
                    }
                }
                "keybind" => {
                    if let Value::Array(keybinds_array) = value {
                        for keybind_val in keybinds_array {
                            if let Ok(input) = keybind_val.try_into::<Input>() {
                                final_config.keybindings.push(input);
                            } else {
                                return Err(format!("Invalid keybind entry in {:?}", path).into());
                            }
                        }
                    } else {
                        return Err(format!(
                            "`keybind` section in {:?} must be an array of tables",
                            path
                        )
                        .into());
                    }
                }
                "prefix" => {
                    if let Value::Array(prefix_array) = value {
                        for prefix_val in prefix_array {
                            if let Ok(input) = prefix_val.try_into::<Prefix>() {
                                final_config.prefixes.push(input);
                            } else {
                                return Err(format!("Invalid prefix entry in {:?}", path).into());
                            }
                        }
                    } else {
                        return Err(format!(
                            "`prefix` section in {:?} must be an array of tables",
                            path
                        )
                        .into());
                    }
                }
                "palette" => {
                    if let Value::Table(palette_table) = value {
                        // Deserialize the palette table directly
                        let current_palette: HashMap<String, String> = palette_table.try_into()?;
                        final_config.palette.extend(current_palette);
                    } else {
                        return Err(
                            format!("`palette` section in {:?} must be a table", path).into()
                        );
                    }
                }
                "theme" => {
                    if let Value::Table(theme_table) = value {
                        // Deserialize the theme table directly
                        let current_theme: HashMap<String, UnresolvedStyle> =
                            theme_table.try_into()?;
                        final_config.theme.extend(current_theme);
                    } else {
                        return Err(format!("`theme` section in {:?} must be a table", path).into());
                    }
                }
                "plugin_config" => {
                    if let Value::Table(value_table) = value {
                        let current_plugin_data: HashMap<String, Value> = value_table.try_into()?;

                        final_config.plugin_config.extend(current_plugin_data);
                    } else {
                        return Err(format!(
                            "`plugin_config` section in {:?} must be a table",
                            path
                        )
                        .into());
                    }
                }
                _ => {
                    // Ignore unknown top-level keys
                    eprintln!(
                        "Warning: Unknown top-level key '{}' found in {:?}",
                        key, path
                    );
                }
            }
        }

        Ok(final_config)
    }

    fn merge(&mut self, other: Config) {
        self.keybindings.extend(other.keybindings);
        self.prefixes.extend(other.prefixes);
        self.palette.extend(other.palette);
        self.theme.extend(other.theme);
        self.plugin_config.extend(other.plugin_config);
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

    pub fn apply(self, state: &mut State) {
        let palette = match self.resolve_palette() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error resolving color palette: {}", e);
                return;
            }
        };

        let mut inputs = state.lock_state::<InputConfig>().unwrap();
        for input in self.keybindings {
            inputs.register_input(kerbin_core::Input {
                valid_modes: input.modes,
                invalid_modes: input.invalid_modes,
                key_sequence: input.keys.iter().map(|x| (x.modifiers, x.code)).collect(),
                event: InputEvent::Commands(input.commands),
                desc: input.desc,
            });
        }

        let mut theme = state.lock_state::<Theme>().unwrap();
        for (name, unresolved_style) in self.theme.into_iter() {
            match unresolved_style.resolve(&palette) {
                Ok(style) => theme.register(name, style),
                Err(e) => eprintln!("Error resolving theme item '{}': {}", name, e),
            }
        }

        let mut prefixes = state.lock_state::<CommandPrefixRegistry>().unwrap();

        for prefix in self.prefixes.into_iter() {
            prefixes.register(CommandPrefix {
                modes: prefix.modes,
                prefix_cmd: prefix.prefix,

                include: prefix.include,
                list: prefix.list,
            });
        }

        state
            .lock_state::<PluginConfig>()
            .unwrap()
            .0
            .extend(self.plugin_config);
    }
}
