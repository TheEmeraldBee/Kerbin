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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ResolvedKeyBind {
    pub mods: KeyModifiers,
    pub code: KeyCode,
}
