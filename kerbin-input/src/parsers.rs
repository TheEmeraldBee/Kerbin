use std::str::FromStr;

use ascii_forge::window::{KeyCode, KeyModifiers};
use thiserror::Error;

use crate::{Matchable, ResolvedKeyBind, UnresolvedKeyBind, UnresolvedKeyElement};

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

impl<T: ParsableKey<Output = T>> ParsableKey for Matchable<T> {
    type Output = Self;
    fn parse_from_str(text: &str) -> Result<Self::Output, ParseError> {
        if text == "*" {
            Ok(Matchable::Any)
        } else {
            Ok(Matchable::Specific(T::parse_from_str(text)?))
        }
    }
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

impl ParsableKey for ResolvedKeyBind {
    type Output = Self;
    fn parse_from_str(text: &str) -> Result<Self::Output, ParseError> {
        // Special case for dash key
        if text == "-" {
            return Ok(ResolvedKeyBind::new_matchable(
                Matchable::Specific(KeyModifiers::empty()),
                Matchable::Specific(KeyCode::Char('-')),
            ));
        }

        let (mods_str, key_str) = if text.ends_with("--") {
            (&text[..text.len() - 2], "-")
        } else {
            match text.rsplit_once('-') {
                Some((m, k)) => (m, k),
                None => ("", text),
            }
        };

        if key_str.is_empty() {
            return Err(ParseError::Custom("Empty key code".to_string()));
        }

        let key_code = Matchable::<KeyCode>::parse_from_str(key_str)?;

        let mut key_mods = Matchable::Specific(KeyModifiers::empty());
        if !mods_str.is_empty() {
            for m in mods_str.split('-') {
                key_mods = key_mods | Matchable::<KeyModifiers>::parse_from_str(m)?;
            }
        }

        Ok(ResolvedKeyBind::new_matchable(key_mods, key_code))
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
impl<'de> Deserialize<'de> for UnresolvedKeyElement<ResolvedKeyBind> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_key_element(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for UnresolvedKeyElement<Matchable<KeyModifiers>> {
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
            '\\' => {
                if let Some(next_char) = chars.next() {
                    current.push(next_char);
                } else {
                    current.push('\\');
                }
            }

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
                    if chars.peek().is_none() {
                        current.push('-');
                    } else {
                        return Err("Empty segment before dash".to_string());
                    }
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

fn parse_key_element(s: &str) -> Result<UnresolvedKeyElement<ResolvedKeyBind>, String> {
    // Check for command substitution first (highest priority)
    if let Some(cmd) = s.strip_prefix("$(").and_then(|s| s.strip_suffix(")")) {
        return parse_command(cmd);
    }

    // Check for OneOf pattern
    if let Some(inner) = s.strip_prefix("(").and_then(|s| s.strip_suffix(")")) {
        if inner.contains('|') {
            let options: Result<Vec<ResolvedKeyBind>, String> = inner
                .split('|')
                .map(|opt| {
                    let opt = opt.trim();
                    ResolvedKeyBind::parse_from_str(opt)
                        .map_err(|e| format!("Failed to parse key '{}': {}", opt, e))
                })
                .collect();
            return Ok(UnresolvedKeyElement::OneOf(options?));
        }

        // Try parsing as literal keybind first
        if let Ok(bind) = ResolvedKeyBind::parse_from_str(inner) {
            return Ok(UnresolvedKeyElement::Literal(bind));
        }

        // It's a template in parentheses
        return Ok(UnresolvedKeyElement::Template(inner.to_string()));
    }

    // Check for template
    if let Some(template_name) = s.strip_prefix('%') {
        return Ok(UnresolvedKeyElement::Template(template_name.to_string()));
    }

    // Otherwise it's a literal
    ResolvedKeyBind::parse_from_str(s)
        .map(UnresolvedKeyElement::Literal)
        .map_err(|e| format!("Failed to parse key '{}': {}", s, e))
}

fn parse_modifier_element(
    s: &str,
) -> Result<UnresolvedKeyElement<Matchable<KeyModifiers>>, String> {
    // Check for command substitution first (highest priority)
    if let Some(cmd) = s.strip_prefix("$(").and_then(|s| s.strip_suffix(")")) {
        return parse_command(cmd);
    }

    // Check for OneOf pattern
    if let Some(inner) = s.strip_prefix("(").and_then(|s| s.strip_suffix(")")) {
        if inner.contains('|') {
            let options: Result<Vec<Matchable<KeyModifiers>>, String> = inner
                .split('|')
                .map(|opt| {
                    let opt = opt.trim();
                    Matchable::<KeyModifiers>::parse_from_str(opt)
                        .map_err(|e| format!("Failed to parse modifier '{}': {}", opt, e))
                })
                .collect();
            return Ok(UnresolvedKeyElement::OneOf(options?));
        }

        // Try parsing as literal modifier first
        if let Ok(mod_) = Matchable::<KeyModifiers>::parse_from_str(inner) {
            return Ok(UnresolvedKeyElement::Literal(mod_));
        }

        // It's a template in parentheses
        return Ok(UnresolvedKeyElement::Template(inner.to_string()));
    }

    // Check for template
    if let Some(template_name) = s.strip_prefix('%') {
        return Ok(UnresolvedKeyElement::Template(template_name.to_string()));
    }

    // Otherwise it's a literal
    Matchable::<KeyModifiers>::parse_from_str(s)
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
            UnresolvedKeyElement::Literal(ResolvedKeyBind {
                code: Matchable::Specific(KeyCode::Char('a')),
                ..
            }) => (),
            _ => panic!("Expected literal 'a'"),
        }
    }

    #[test]
    fn test_parse_nested_key() {
        // alt-(shift-a)
        let bind: UnresolvedKeyBind = "alt-(shift-a)".parse().unwrap();
        assert_eq!(bind.mods.len(), 1);
        match &bind.code {
            UnresolvedKeyElement::Literal(ResolvedKeyBind {
                code: Matchable::Specific(KeyCode::Char('a')),
                mods: Matchable::Specific(mods),
            }) => {
                assert!(mods.contains(KeyModifiers::SHIFT));
            }
            _ => panic!("Expected literal 'shift-a' inside code"),
        }
    }

    #[test]
    fn test_parse_wildcard_mod() {
        let bind: UnresolvedKeyBind = "*-a".parse().unwrap();
        assert_eq!(bind.mods.len(), 1);
        match &bind.mods[0] {
            UnresolvedKeyElement::Literal(Matchable::Any) => (),
            _ => panic!("Expected wildcard mod"),
        }
    }

    #[test]
    fn test_parse_wildcard_key() {
        let bind: UnresolvedKeyBind = "ctrl-*".parse().unwrap();
        match &bind.code {
            UnresolvedKeyElement::Literal(ResolvedKeyBind {
                code: Matchable::Any,
                mods: Matchable::Specific(m),
            }) => {
                // "ctrl" should be in outer mods, so inner mods usually empty
                // unless parsed as "ctrl-*" from a single string.
                // Wait, "ctrl-*" is parsed. "ctrl" is mod, "*" is key.
            }
            _ => panic!("Expected wildcard key"),
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
    fn test_parse_dash() {
        let bind: UnresolvedKeyBind = "-".parse().unwrap();
        match &bind.code {
            UnresolvedKeyElement::Literal(ResolvedKeyBind {
                code: Matchable::Specific(KeyCode::Char('-')),
                ..
            }) => (),
            _ => panic!("Expected literal '-'"),
        }
    }

    #[test]
    fn test_parse_ctrl_dash() {
        let bind: UnresolvedKeyBind = "ctrl--".parse().unwrap();
        assert_eq!(bind.mods.len(), 1);
        match &bind.code {
            UnresolvedKeyElement::Literal(ResolvedKeyBind {
                code: Matchable::Specific(KeyCode::Char('-')),
                ..
            }) => (),
            _ => panic!("Expected literal '-'"),
        }
    }

    #[test]
    fn test_parse_escaped_dash() {
        // "ctrl-\-" should be parsed as "ctrl" modifier and "-" key
        let bind: UnresolvedKeyBind = r"ctrl-\-".parse().unwrap();
        assert_eq!(bind.mods.len(), 1);
        match &bind.code {
            UnresolvedKeyElement::Literal(ResolvedKeyBind {
                code: Matchable::Specific(KeyCode::Char('-')),
                ..
            }) => (),
            _ => panic!("Expected literal '-'"),
        }
    }
}
