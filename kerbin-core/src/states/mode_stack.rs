use crate::*;

/// State representing the current mode stack of the editor.
///
/// The mode stack determines the current operational context of the editor,
/// influencing keybindings, command prefixes, and other behaviors.
/// The bottom of the stack is typically 'n' for normal mode.
#[derive(State)]
pub struct ModeStack(pub Vec<char>);

impl ModeStack {
    /// Pushes a new mode onto the mode stack.
    ///
    /// The newly pushed mode becomes the current active mode.
    pub fn push_mode(&mut self, mode: char) {
        self.0.push(mode);
    }

    /// Pops the top mode from the mode stack.
    ///
    /// If only one mode remains (typically 'n' for normal mode), it cannot be popped
    /// to ensure there's always an active mode.
    pub fn pop_mode(&mut self) -> Option<char> {
        if self.0.len() <= 1 {
            return None;
        }

        self.0.pop()
    }

    /// Sets the current mode, clearing all other modes and ensuring 'n' (normal mode)
    /// is at the bottom of the stack, followed by the specified mode if it's not 'n'.
    ///
    /// This effectively switches the editor to a new, single-active mode.
    pub fn set_mode(&mut self, mode: char) {
        self.0.clear();
        self.0.push('n');
        // Since we already pushed normal mode.
        if mode == 'n' {
            return;
        }
        self.0.push(mode);
    }

    /// Returns the current active mode (the top-most mode on the stack).
    pub fn get_mode(&self) -> char {
        *self.0.last().unwrap()
    }

    /// Checks if a given mode is currently present anywhere on the mode stack.
    ///
    /// This is useful for determining if the editor is in a specific mode,
    /// even if it's not the top-most (current) mode.
    pub fn mode_on_stack(&self, mode: char) -> bool {
        self.0.contains(&mode)
    }

    /// Locates the index of the stack that the mode is on, in decending order
    ///
    /// IE: 'n' -> 'i'
    /// Would be (1 -> 0)
    /// So asking for 'n' would return 1,
    /// asking for 'i' would return 0
    pub fn where_on_stack(&self, mode: char) -> Option<usize> {
        self.0
            .iter()
            .rev()
            .enumerate()
            .find(|x| *x.1 == mode)
            .map(|x| x.0)
    }
}
