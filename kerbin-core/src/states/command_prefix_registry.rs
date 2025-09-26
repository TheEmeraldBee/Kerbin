use crate::*;

/// State used for storing registered command prefixes.
///
/// Contains a vector of `CommandPrefix` configurations.
#[derive(State)]
pub struct CommandPrefixRegistry(pub Vec<CommandPrefix>);

impl CommandPrefixRegistry {
    /// Registers a new command prefix configuration.
    ///
    /// This adds a `CommandPrefix` to the registry, which can then be used
    /// by the `CommandRegistry` to modify user input based on active modes.
    ///
    /// # Arguments
    ///
    /// * `prefix`: The `CommandPrefix` to register.
    pub fn register(&mut self, prefix: CommandPrefix) {
        self.0.push(prefix)
    }
}
