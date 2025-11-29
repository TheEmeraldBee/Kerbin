use std::fmt::Display;

use ascii_forge::window::{KeyCode, KeyModifiers};

use crate::ParsableKey;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum UnresolvedKeyElement<T: ParsableKey<Output = T>> {
    Literal(T),
    OneOf(Vec<T>),
    Template(String),
    Command(String, Vec<String>),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct UnresolvedKeyBind {
    pub mods: Vec<UnresolvedKeyElement<KeyModifiers>>,
    pub code: UnresolvedKeyElement<KeyCode>,
}

impl Display for UnresolvedKeyBind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KeyCode::*;
        use KeyModifiers as M;

        let mut parts = Vec::new();

        let has_shift = self.mods.iter().any(|m| match m {
            UnresolvedKeyElement::Literal(mods) => mods.contains(M::SHIFT),
            _ => false,
        });

        for m in &self.mods {
            let mut include = true;

            if let UnresolvedKeyElement::Literal(mods) = m
                && mods.contains(M::SHIFT)
                && let UnresolvedKeyElement::Literal(Char(_)) = &self.code
            {
                include = false;
            }

            if include {
                parts.push(m.to_string());
            }
        }

        let code_str = match &self.code {
            UnresolvedKeyElement::Literal(Char(ch)) => {
                if has_shift {
                    ch.to_ascii_uppercase().to_string()
                } else {
                    ch.to_ascii_lowercase().to_string()
                }
            }
            other => other.to_string(),
        };

        if parts.is_empty() {
            write!(f, "{}", code_str)
        } else {
            write!(f, "{}-{}", parts.join("-"), code_str)
        }
    }
}

impl<T: ParsableKey<Output = T> + Display> Display for UnresolvedKeyElement<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnresolvedKeyElement::Literal(v) => {
                let s = v.to_string();
                // Capitalize modifier names to match ResolvedKeyBind format
                let formatted = match s.to_lowercase().as_str() {
                    "control" => "Ctrl".to_string(),
                    "alt" => "Alt".to_string(),
                    "super" => "Super".to_string(),
                    "shift" => "Shift".to_string(),
                    _ => s.to_lowercase(),
                };
                write!(f, "{}", formatted)
            }
            UnresolvedKeyElement::OneOf(vs) => {
                let opts = vs
                    .iter()
                    .map(|v| {
                        let s = v.to_string();
                        match s.to_lowercase().as_str() {
                            "control" => "Ctrl".to_string(),
                            "alt" => "Alt".to_string(),
                            "super" => "Super".to_string(),
                            "shift" => "Shift".to_string(),
                            _ => s.to_lowercase(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("|");
                write!(f, "({})", opts)
            }
            UnresolvedKeyElement::Template(t) => write!(f, "%{}", t),
            UnresolvedKeyElement::Command(cmd, args) => {
                if args.is_empty() {
                    write!(f, "$({})", cmd)
                } else {
                    write!(f, "$({} {})", cmd, args.join(" "))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ResolvedKeyBind {
    pub mods: KeyModifiers,
    pub code: KeyCode,
}

impl ResolvedKeyBind {
    pub fn new(mut mods: KeyModifiers, mut code: KeyCode) -> Self {
        if let KeyCode::Char(ch) = code {
            if ch.is_ascii_uppercase() {
                mods = mods.union(KeyModifiers::SHIFT);
            }

            code = KeyCode::Char(ch.to_ascii_lowercase())
        }

        Self { mods, code }
    }
}

impl Display for ResolvedKeyBind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KeyCode::*;
        use KeyModifiers as M;

        let mut parts = Vec::new();
        let mods = self.mods;
        let code = &self.code;

        let key_str = match code {
            Char(ch) => {
                if mods.contains(M::SHIFT) {
                    ch.to_ascii_uppercase().to_string()
                } else {
                    ch.to_ascii_lowercase().to_string()
                }
            }
            _ => code.to_string().to_lowercase(),
        };

        if mods.contains(M::CONTROL) {
            parts.push("Ctrl".to_string());
        }
        if mods.contains(M::ALT) {
            parts.push("Alt".to_string());
        }
        if mods.contains(M::SUPER) {
            parts.push("Super".to_string());
        }
        if mods.contains(M::SHIFT) && !matches!(code, Char('A'..='Z') | Char('a'..='z')) {
            parts.push("Shift".to_string());
        }

        if parts.is_empty() {
            write!(f, "{}", key_str)
        } else {
            write!(f, "{}-{}", parts.join("-"), key_str)
        }
    }
}
