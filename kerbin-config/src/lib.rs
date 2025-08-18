use ascii_forge::prelude::{KeyCode, KeyModifiers};
use kerbin_core::{InputEvent, State};
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use std::fmt;
use std::fs;
use std::sync::Arc;

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
pub struct Config {
    #[serde(rename = "keybind")]
    keybindings: Vec<Input>,
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
    }
}
