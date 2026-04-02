use crate::*;

/// State representing the current mode stack of the editor
#[derive(State)]
pub struct ModeStack(pub Vec<char>);

impl ModeStack {
    pub fn push_mode(&mut self, mode: char) {
        self.0.push(mode);
    }

    pub fn pop_mode(&mut self) -> Option<char> {
        if self.0.len() <= 1 {
            return None;
        }

        self.0.pop()
    }

    pub fn set_mode(&mut self, mode: char) {
        self.0.clear();
        self.0.push('n');
        if mode == 'n' {
            return;
        }
        self.0.push(mode);
    }

    pub fn get_mode(&self) -> char {
        // Stack always has at least 'n' (normal mode) — pop_mode guards the minimum
        *self.0.last().expect("mode stack is never empty")
    }

    /// Checks if a given mode is currently present anywhere on the mode stack
    pub fn mode_on_stack(&self, mode: char) -> bool {
        self.0.contains(&mode)
    }

    /// Returns the depth of `mode` from the top of the stack (0 = top), or `None`
    pub fn where_on_stack(&self, mode: char) -> Option<usize> {
        self.0
            .iter()
            .rev()
            .enumerate()
            .find(|x| *x.1 == mode)
            .map(|x| x.0)
    }
}
