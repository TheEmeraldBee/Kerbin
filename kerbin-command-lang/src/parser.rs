use std::collections::BTreeMap;

use crate::Token;

pub struct CommandState {
    pub name: String,
    pub positional: Vec<Token>,
    pub flags: BTreeMap<String, Option<Token>>,
}

impl CommandState {
    pub fn parse(val: &[Token]) -> Option<Self> {
        let name = match val.get(0) {
            Some(Token::Word(s)) => s.clone(),
            _ => return None,
        };

        let mut positional = Vec::new();
        let mut flags: BTreeMap<String, Option<Token>> = BTreeMap::new();
        let mut i = 1usize;

        while i < val.len() {
            match &val[i] {
                Token::Word(s) if s.starts_with("--") => {
                    let flag_name = s.clone();
                    let has_value = match val.get(i + 1) {
                        Some(Token::Word(v)) if !v.starts_with("--") => true,
                        Some(Token::List(_)) => true,
                        _ => false,
                    };
                    if has_value {
                        flags.insert(flag_name, Some(val[i + 1].clone()));
                        i += 2;
                    } else {
                        flags.insert(flag_name, None);
                        i += 1;
                    }
                }
                other => {
                    positional.push(other.clone());
                    i += 1;
                }
            }
        }

        Some(CommandState {
            name,
            positional,
            flags,
        })
    }
}
