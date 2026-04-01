use crate::*;

/// Metadata for a registered plugin.
pub struct PluginInfo {
    pub name: &'static str,
}

/// Stores metadata for all plugins that have called `init`.
/// Populated automatically by `define_plugin!` when a `name` is provided.
#[derive(State, Default)]
pub struct PluginRegistry(pub Vec<PluginInfo>);

impl PluginRegistry {
    pub fn register(&mut self, name: &'static str) {
        self.0.push(PluginInfo { name });
    }
}
