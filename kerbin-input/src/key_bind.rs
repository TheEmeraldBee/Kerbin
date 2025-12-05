use std::fmt::Display;
use std::ops::BitOr;

use ascii_forge::window::{KeyCode, KeyModifiers};

use crate::ParsableKey;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Copy)]
pub enum Matchable<T> {
    Specific(T),
    Any,
}

impl<T: Display> Display for Matchable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Matchable::Specific(v) => write!(f, "{}", v),
            Matchable::Any => write!(f, "*"),
        }
    }
}

impl<T: BitOr<Output = T>> BitOr for Matchable<T> {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Matchable::Specific(a), Matchable::Specific(b)) => Matchable::Specific(a | b),
            _ => Matchable::Any,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum UnresolvedKeyElement<T: ParsableKey<Output = T>> {
    Literal(T),
    OneOf(Vec<T>),
    Template(String),
    Command(String, Vec<String>),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct UnresolvedKeyBind {
    pub mods: Vec<UnresolvedKeyElement<Matchable<KeyModifiers>>>,
    pub code: UnresolvedKeyElement<ResolvedKeyBind>,
}

impl Display for UnresolvedKeyBind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KeyCode::*;
        use KeyModifiers as M;

        let mut parts = Vec::new();

        let has_shift = self.mods.iter().any(|m| match m {
            UnresolvedKeyElement::Literal(Matchable::Specific(mods)) => mods.contains(M::SHIFT),
            UnresolvedKeyElement::Literal(Matchable::Any) => false, // Or true?
            _ => false,
        });

        for m in &self.mods {
            let mut include = true;

            if let UnresolvedKeyElement::Literal(Matchable::Specific(mods)) = m
                && mods.contains(M::SHIFT)
                && let UnresolvedKeyElement::Literal(ResolvedKeyBind {
                    code: Matchable::Specific(Char(_)),
                    mods: Matchable::Specific(inner_mods),
                }) = &self.code
                && inner_mods.is_empty()
            {
                include = false;
            }

            if include {
                parts.push(m.to_string());
            }
        }

        let code_str = match &self.code {
            UnresolvedKeyElement::Literal(ResolvedKeyBind {
                code: Matchable::Specific(Char(ch)),
                mods: Matchable::Specific(mods),
            }) if mods.is_empty() => {
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
    pub mods: Matchable<KeyModifiers>,
    pub code: Matchable<KeyCode>,
}

impl ResolvedKeyBind {
    pub fn new(mut mods: KeyModifiers, mut code: KeyCode) -> Self {
        if let KeyCode::Char(ch) = code {
            if ch.is_ascii_uppercase() {
                mods = mods.union(KeyModifiers::SHIFT);
            }

            code = KeyCode::Char(ch.to_ascii_lowercase())
        }

        Self {
            mods: Matchable::Specific(mods),
            code: Matchable::Specific(code),
        }
    }

    pub fn new_matchable(mods: Matchable<KeyModifiers>, code: Matchable<KeyCode>) -> Self {
        let (final_mods, final_code) = match (mods, code) {
            (Matchable::Specific(mut m), Matchable::Specific(mut c)) => {
                if let KeyCode::Char(ch) = c {
                    if ch.is_ascii_uppercase() {
                        m = m.union(KeyModifiers::SHIFT);
                    }
                    c = KeyCode::Char(ch.to_ascii_lowercase())
                }
                (Matchable::Specific(m), Matchable::Specific(c))
            }
            (m, c) => (m, c),
        };

        Self {
            mods: final_mods,
            code: final_code,
        }
    }
}

impl Display for ResolvedKeyBind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KeyCode::*;
        use KeyModifiers as M;

        let mut parts = Vec::new();
        let mods = self.mods;
        let code = self.code;

        match mods {
            Matchable::Specific(m) => {
                if m.contains(M::CONTROL) {
                    parts.push("Ctrl".to_string());
                }
                if m.contains(M::ALT) {
                    parts.push("Alt".to_string());
                }
                if m.contains(M::SUPER) {
                    parts.push("Super".to_string());
                }
                if m.contains(M::SHIFT) {
                    let implicit_shift = if let Matchable::Specific(Char(c)) = code {
                        !c.is_ascii_uppercase() && !c.is_ascii_lowercase()
                    } else {
                        false
                    };

                    if !implicit_shift {
                        // Check if it's a letter
                        if let Matchable::Specific(Char(c)) = code {
                            if !c.is_ascii_alphabetic() {
                                parts.push("Shift".to_string());
                            }
                        } else {
                            parts.push("Shift".to_string());
                        }
                    }
                }
            }
            Matchable::Any => parts.push("*".to_string()),
        }

        let key_str = match code {
            Matchable::Specific(Char(ch)) => {
                let shift = match mods {
                    Matchable::Specific(m) => m.contains(M::SHIFT),
                    _ => false,
                };
                if shift {
                    ch.to_ascii_uppercase().to_string()
                } else {
                    ch.to_ascii_lowercase().to_string()
                }
            }
            Matchable::Specific(c) => c.to_string().to_lowercase(),
            Matchable::Any => "*".to_string(),
        };

        if parts.is_empty() {
            write!(f, "{}", key_str)
        } else {
            write!(f, "{}-{}", parts.join("-"), key_str)
        }
    }
}
