pub mod commands;
pub use commands::*;
pub mod interceptor;
pub use interceptor::*;

use crate::*;

#[derive(Clone, Debug)]
pub struct BracketPair {
    pub open: char,
    pub close: char,
}

#[derive(State)]
pub struct AutoPairs {
    pub pairs: Vec<BracketPair>,
}

impl Default for AutoPairs {
    fn default() -> Self {
        Self {
            pairs: vec![
                BracketPair {
                    open: '(',
                    close: ')',
                },
                BracketPair {
                    open: '[',
                    close: ']',
                },
                BracketPair {
                    open: '{',
                    close: '}',
                },
                BracketPair {
                    open: '<',
                    close: '>',
                },
                BracketPair {
                    open: '"',
                    close: '"',
                },
                BracketPair {
                    open: '\'',
                    close: '\'',
                },
            ],
        }
    }
}

impl AutoPairs {
    pub fn add_pair(&mut self, open: char, close: char) {
        self.pairs.retain(|p| p.open != open);
        self.pairs.push(BracketPair { open, close });
    }

    pub fn remove_pair(&mut self, open: char) {
        self.pairs.retain(|p| p.open != open);
    }

    pub fn find_by_open(&self, c: char) -> Option<&BracketPair> {
        self.pairs.iter().find(|p| p.open == c)
    }

    pub fn find_by_close(&self, c: char) -> Option<&BracketPair> {
        self.pairs.iter().find(|p| p.close == c)
    }
}
