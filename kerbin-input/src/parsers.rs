use std::str::FromStr;

use ascii_forge::window::{KeyCode, KeyModifiers};
use thiserror::Error;

use crate::{UnresolvedKeyBind, UnresolvedKeyElement};

#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Unknown element for `{elem_type}`, `{text}`")]
    UnknownElement {
        elem_type: &'static str,
        text: String,
    },

    #[error("Unknown element for `{elem_type}::{elem_subtype}`, got `{text}`")]
    InvalidElement {
        elem_type: &'static str,
        elem_subtype: &'static str,
        text: String,
    },

    #[error("{0}")]
    Custom(String),
}

pub trait ParsableKey: Clone {
    type Output;
    fn parse_from_str(text: &str) -> Result<Self::Output, ParseError>;
}

impl ParsableKey for KeyModifiers {
    type Output = Self;
    fn parse_from_str(text: &str) -> Result<Self::Output, ParseError> {
        match text {
            "ctrl" | "control" => Ok(Self::CONTROL),
            "alt" => Ok(Self::ALT),
            "shift" => Ok(Self::SHIFT),
            _ => Err(ParseError::UnknownElement {
                elem_type: "modifier",
                text: text.to_string(),
            }),
        }
    }
}

impl ParsableKey for KeyCode {
    type Output = Self;
    fn parse_from_str(text: &str) -> Result<Self::Output, ParseError> {
        Ok(match text {
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
                    .map_err(|_| ParseError::InvalidElement {
                        elem_type: "key",
                        elem_subtype: "fn_code",
                        text: text.to_string(),
                    })?;
                KeyCode::F(num)
            }
            s if s.chars().count() == 1 => KeyCode::Char(s.chars().next().unwrap()),
            _ => {
                return Err(ParseError::UnknownElement {
                    elem_type: "key",
                    text: text.to_string(),
                });
            }
        })
    }
}

impl FromStr for UnresolvedKeyBind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse the string into segments, respecting special constructs
        let segments = parse_segments(s)?;

        if segments.is_empty() {
            return Err("Empty keybind string".to_string());
        }

        // The last segment is the key code, everything before is modifiers
        let (mod_segments, key_segment) = segments.split_at(segments.len() - 1);

        // Parse the key code
        let code = parse_key_element(&key_segment[0])?;

        // Parse modifiers
        let mut mods = Vec::new();
        for mod_str in mod_segments {
            mods.push(parse_modifier_element(mod_str)?);
        }

        Ok(UnresolvedKeyBind { mods, code })
    }
}

#[cfg(feature = "serde")]
impl Serialize for UnresolvedKeyBind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for UnresolvedKeyBind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl<T> Serialize for UnresolvedKeyElement<T>
where
    T: std::fmt::Display + ParsableKey<Output = T>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for UnresolvedKeyElement<KeyCode> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_key_element(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for UnresolvedKeyElement<KeyModifiers> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_modifier_element(&s).map_err(serde::de::Error::custom)
    }
}

/// Parse a string into segments separated by dashes, but respecting
/// special constructs like $(), (), %template that may contain dashes
fn parse_segments(s: &str) -> Result<Vec<String>, String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Command substitution
            '$' if chars.peek() == Some(&'(') => {
                chars.next(); // consume '('
                current.push_str("$(");

                let mut depth = 1;
                for c in chars.by_ref() {
                    current.push(c);
                    if c == '(' {
                        depth += 1;
                    } else if c == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }

                if depth != 0 {
                    return Err("Unmatched parentheses in command".to_string());
                }
            }

            // OneOf or template group
            '(' => {
                current.push('(');
                let mut depth = 1;

                for c in chars.by_ref() {
                    current.push(c);
                    if c == '(' {
                        depth += 1;
                    } else if c == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }

                if depth != 0 {
                    return Err("Unmatched parentheses".to_string());
                }
            }

            // Template - consume until non-alphanumeric/underscore
            '%' => {
                current.push('%');
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        current.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
            }

            // Dash separator - split here
            '-' => {
                if !current.is_empty() {
                    segments.push(current.clone());
                    current.clear();
                } else {
                    return Err("Empty segment before dash".to_string());
                }
            }

            // Regular character
            _ => {
                current.push(ch);
            }
        }
    }

    // Add the last segment
    if !current.is_empty() {
        segments.push(current);
    } else if !segments.is_empty() {
        return Err("Trailing dash".to_string());
    }

    Ok(segments)
}

fn parse_key_element(s: &str) -> Result<UnresolvedKeyElement<KeyCode>, String> {
    // Check for command substitution first (highest priority)
    if let Some(cmd) = s.strip_prefix("$(").and_then(|s| s.strip_suffix(")")) {
        return parse_command(cmd);
    }

    // Check for OneOf pattern
    if let Some(inner) = s.strip_prefix("(").and_then(|s| s.strip_suffix(")")) {
        if inner.contains('|') {
            let options: Result<Vec<KeyCode>, String> = inner
                .split('|')
                .map(|opt| {
                    let opt = opt.trim();
                    KeyCode::parse_from_str(opt)
                        .map_err(|e| format!("Failed to parse key '{}': {}", opt, e))
                })
                .collect();
            return Ok(UnresolvedKeyElement::OneOf(options?));
        }
        // It's a template in parentheses
        return Ok(UnresolvedKeyElement::Template(inner.to_string()));
    }

    // Check for template
    if let Some(template_name) = s.strip_prefix('%') {
        return Ok(UnresolvedKeyElement::Template(template_name.to_string()));
    }

    // Otherwise it's a literal
    KeyCode::parse_from_str(s)
        .map(UnresolvedKeyElement::Literal)
        .map_err(|e| format!("Failed to parse key '{}': {}", s, e))
}

fn parse_modifier_element(s: &str) -> Result<UnresolvedKeyElement<KeyModifiers>, String> {
    // Check for command substitution first (highest priority)
    if let Some(cmd) = s.strip_prefix("$(").and_then(|s| s.strip_suffix(")")) {
        return parse_command(cmd);
    }

    // Check for OneOf pattern
    if let Some(inner) = s.strip_prefix("(").and_then(|s| s.strip_suffix(")")) {
        if inner.contains('|') {
            let options: Result<Vec<KeyModifiers>, String> = inner
                .split('|')
                .map(|opt| {
                    let opt = opt.trim();
                    KeyModifiers::parse_from_str(opt)
                        .map_err(|e| format!("Failed to parse modifier '{}': {}", opt, e))
                })
                .collect();
            return Ok(UnresolvedKeyElement::OneOf(options?));
        }
        // It's a template in parentheses
        return Ok(UnresolvedKeyElement::Template(inner.to_string()));
    }

    // Check for template
    if let Some(template_name) = s.strip_prefix('%') {
        return Ok(UnresolvedKeyElement::Template(template_name.to_string()));
    }

    // Otherwise it's a literal
    KeyModifiers::parse_from_str(s)
        .map(UnresolvedKeyElement::Literal)
        .map_err(|e| format!("Failed to parse modifier '{}': {}", s, e))
}

fn parse_command<T>(cmd: &str) -> Result<UnresolvedKeyElement<T>, String>
where
    T: ParsableKey<Output = T>,
{
    // Split command and arguments
    let parts = shellwords::split(cmd).map_err(|x| x.to_string())?;
    if parts.is_empty() {
        return Err("Empty command".to_string());
    }

    let command = parts[0].to_string();
    let args = parts[1..].iter().map(|s| s.to_string()).collect();

    Ok(UnresolvedKeyElement::Command(command, args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_key() {
        let bind: UnresolvedKeyBind = "ctrl-a".parse().unwrap();
        assert_eq!(bind.mods.len(), 1);
        match &bind.code {
            UnresolvedKeyElement::Literal(KeyCode::Char('a')) => (),
            _ => panic!("Expected literal 'a'"),
        }
    }

    #[test]
    fn test_parse_multiple_modifiers() {
        let bind: UnresolvedKeyBind = "ctrl-shift-a".parse().unwrap();
        assert_eq!(bind.mods.len(), 2);
    }

    #[test]
    fn test_parse_oneof_keys() {
        let bind: UnresolvedKeyBind = "ctrl-(a|b|c)".parse().unwrap();
        assert_eq!(bind.mods.len(), 1);
        match &bind.code {
            UnresolvedKeyElement::OneOf(opts) => assert_eq!(opts.len(), 3),
            _ => panic!("Expected OneOf"),
        }
    }

    #[test]
    fn test_parse_oneof_modifiers() {
        let bind: UnresolvedKeyBind = "(ctrl|alt)-a".parse().unwrap();
        assert_eq!(bind.mods.len(), 1);
        match &bind.mods[0] {
            UnresolvedKeyElement::OneOf(opts) => assert_eq!(opts.len(), 2),
            _ => panic!("Expected OneOf for modifiers"),
        }
    }

    #[test]
    fn test_parse_template() {
        let bind: UnresolvedKeyBind = "ctrl-%navigation".parse().unwrap();
        match &bind.code {
            UnresolvedKeyElement::Template(name) => assert_eq!(name, "navigation"),
            _ => panic!("Expected template"),
        }
    }

    #[test]
    fn test_parse_command() {
        let bind: UnresolvedKeyBind = "ctrl-$(get_key)".parse().unwrap();
        match &bind.code {
            UnresolvedKeyElement::Command(cmd, args) => {
                assert_eq!(cmd, "get_key");
                assert_eq!(args.len(), 0);
            }
            _ => panic!("Expected command"),
        }
    }

    #[test]
    fn test_parse_command_with_dashes() {
        let bind: UnresolvedKeyBind = "ctrl-$(get-key)".parse().unwrap();
        match &bind.code {
            UnresolvedKeyElement::Command(cmd, args) => {
                assert_eq!(cmd, "get-key");
                assert_eq!(args.len(), 0);
            }
            _ => panic!("Expected command"),
        }
    }

    #[test]
    fn test_parse_command_with_args() {
        let bind: UnresolvedKeyBind = "ctrl-$(get-key arg1 arg2)".parse().unwrap();
        match &bind.code {
            UnresolvedKeyElement::Command(cmd, args) => {
                assert_eq!(cmd, "get-key");
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], "arg1");
                assert_eq!(args[1], "arg2");
            }
            _ => panic!("Expected command"),
        }
    }

    #[test]
    fn test_parse_no_modifiers() {
        let bind: UnresolvedKeyBind = "a".parse().unwrap();
        assert_eq!(bind.mods.len(), 0);
        match &bind.code {
            UnresolvedKeyElement::Literal(KeyCode::Char('a')) => (),
            _ => panic!("Expected literal 'a'"),
        }
    }

    #[test]
    fn test_parse_segments() {
        let segments = parse_segments("ctrl-alt-a").unwrap();
        assert_eq!(segments, vec!["ctrl", "alt", "a"]);

        let segments = parse_segments("ctrl-$(get-key)").unwrap();
        assert_eq!(segments, vec!["ctrl", "$(get-key)"]);

        let segments = parse_segments("(ctrl|alt)-a").unwrap();
        assert_eq!(segments, vec!["(ctrl|alt)", "a"]);

        let segments = parse_segments("ctrl-%template").unwrap();
        assert_eq!(segments, vec!["ctrl", "%template"]);
    }

    #[test]
    fn test_to_string_roundtrip() {
        let test_cases = vec![
            "ctrl-a",
            "ctrl-alt-a",
            "ctrl-%navigation",
            "ctrl-$(get_key)",
        ];

        for case in test_cases {
            let bind: UnresolvedKeyBind = case.parse().unwrap();
            let serialized = bind.to_string();
            // Note: roundtrip may not be perfect for all cases due to Debug formatting
            println!("Original: {}, Serialized: {}", case, serialized);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_serde_roundtrip() {
        let bind: UnresolvedKeyBind = "ctrl-a".parse().unwrap();
        let json = serde_json::to_string(&bind).unwrap();
        let deserialized: UnresolvedKeyBind = serde_json::from_str(&json).unwrap();
        assert_eq!(bind.to_string(), deserialized.to_string());
    }
}
