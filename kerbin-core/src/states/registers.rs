use std::collections::HashMap;

use crate::*;

/// Registers are char-indexed sets of stored text
#[derive(State)]
pub struct Registers {
    last_used: char,
    registers: HashMap<char, String>,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            last_used: '"',
            registers: HashMap::default(),
        }
    }
}

impl Registers {
    /// Returns the last used register
    pub fn last_used(&self) -> char {
        self.last_used
    }

    /// Sets a register's text to the given value
    pub fn set(&mut self, register: char, text: String) {
        self.last_used = register;

        self.registers.insert(register, text);
    }

    /// Returns a register's text
    pub fn get(&mut self, register: &char) -> &str {
        self.last_used = *register;

        self.registers
            .get(register)
            .map(|x| x.as_str())
            .unwrap_or("")
    }
}
