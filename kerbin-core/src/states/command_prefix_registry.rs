use crate::*;

/// State for storing registered command prefixes
#[derive(State)]
pub struct CommandPrefixRegistry(pub Vec<CommandPrefix>);

impl CommandPrefixRegistry {
    /// Registers a new command prefix configuration
    pub fn register(&mut self, prefix: CommandPrefix) {
        self.0.push(prefix)
    }

    /// Clears all registered command prefixes.
    pub fn clear(&mut self) {
        self.0.clear();
    }
}
