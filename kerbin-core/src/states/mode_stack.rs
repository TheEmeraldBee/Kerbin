use crate::*;

/// State representing the current mode stack of the editor
#[derive(State)]
pub struct ModeStack(pub Vec<char>);

impl ModeStack {
    /// Pushes a new mode onto the mode stack
    pub fn push_mode(&mut self, mode: char) {
        self.0.push(mode);
    }

    /// Pops the top mode from the mode stack
    pub fn pop_mode(&mut self) -> Option<char> {
        if self.0.len() <= 1 {
            return None;
        }

        self.0.pop()
    }

    /// Sets the current mode
    pub fn set_mode(&mut self, mode: char) {
        self.0.clear();
        self.0.push('n');
        // Since we already pushed normal mode.
        if mode == 'n' {
            return;
        }
        self.0.push(mode);
    }

    /// Returns the current active mode
    pub fn get_mode(&self) -> char {
        *self.0.last().unwrap()
    }

    /// Checks if a given mode is currently present anywhere on the mode stack
    pub fn mode_on_stack(&self, mode: char) -> bool {
        self.0.contains(&mode)
    }

    /// Locates the index of the stack that the mode is on in descending order
    pub fn where_on_stack(&self, mode: char) -> Option<usize> {
        self.0
            .iter()
            .rev()
            .enumerate()
            .find(|x| *x.1 == mode)
            .map(|x| x.0)
    }
}
