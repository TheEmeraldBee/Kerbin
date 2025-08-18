use ascii_forge::prelude::*;
use kerbin_core::{InputEvent, State};
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::sync::Arc;

fn deserialize_color<E>(value: &str) -> Result<Color, E>
where
    E: de::Error,
{
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
            let r =
                u8::from_str_radix(&s[1..3], 16).map_err(|_| E::custom("Invalid hex for red"))?;
            let g =
                u8::from_str_radix(&s[3..5], 16).map_err(|_| E::custom("Invalid hex for green"))?;
            let b =
                u8::from_str_radix(&s[5..7], 16).map_err(|_| E::custom("Invalid hex for blue"))?;
            Ok(Color::Rgb { r, g, b })
        }
        _ => Err(E::custom(format!("Unknown color: {}", value))),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InnerStyle(ContentStyle);

impl InnerStyle {
    pub fn to_style(self) -> ContentStyle {
        self.0
    }
}

impl<'de> Deserialize<'de> for InnerStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ContentStyleVisitor;

        impl<'de> Visitor<'de> for ContentStyleVisitor {
            type Value = InnerStyle;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a style table with 'fg', 'bg', 'underline', and 'attrs'")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut style = ContentStyle::new();
                let mut attrs = Attributes::none();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "fg" => {
                            let color_str: String = map.next_value()?;
                            style.foreground_color = Some(deserialize_color(&color_str)?);
                        }
                        "bg" => {
                            let color_str: String = map.next_value()?;
                            style.background_color = Some(deserialize_color(&color_str)?);
                        }
                        "underline" => {
                            let color_str: String = map.next_value()?;
                            style.underline_color = Some(deserialize_color(&color_str)?);
                        }
                        "attrs" => {
                            let attr_list: Vec<String> = map.next_value()?;
                            for attr_str in attr_list {
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
                                    _ => {
                                        return Err(de::Error::custom(format!(
                                            "Unknown attribute: {}",
                                            attr_str
                                        )));
                                    }
                                };
                                attrs.set(attr);
                            }
                        }
                        _ => {
                            let _: serde_value::Value = map.next_value()?;
                        }
                    }
                }
                style.attributes = attrs;
                Ok(InnerStyle(style))
            }
        }

        deserializer.deserialize_map(ContentStyleVisitor)
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

#[derive(Deserialize, Debug)]
pub struct Input {
    pub modes: Vec<char>,
    pub keys: Vec<KeyBind>,
    pub command: String,
    pub desc: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(rename = "keybind")]
    keybindings: Vec<Input>,

    theme: HashMap<String, InnerStyle>,
}

impl Config {
    pub fn load(path: Option<String>) -> Result<Self, toml::de::Error> {
        let file_content = fs::read_to_string(path.unwrap_or("config.toml".to_string()))
            .expect("Could not read config file");
        toml::from_str(&file_content)
    }

    pub fn apply(self, state: Arc<State>) {
        // Register inputs
        let mut inputs = state.input_config.write().unwrap();
        for input in self.keybindings {
            inputs.register_input(kerbin_core::Input {
                valid_modes: input.modes,
                key_sequence: input.keys.iter().map(|x| (x.modifiers, x.code)).collect(),
                event: InputEvent::Command(input.command),
                desc: input.desc,
            });
        }

        let mut theme = state.theme.write().unwrap();
        for (name, style) in self.theme.into_iter() {
            theme.register(name, style.to_style());
        }
    }
}
