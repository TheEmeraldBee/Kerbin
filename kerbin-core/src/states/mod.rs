pub mod wrappers;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use ascii_forge::window::Window;
use tokio::sync::mpsc::UnboundedSender;
pub use wrappers::*;

pub mod command_registry;
pub use command_registry::*;

pub mod command_prefix_registry;
pub use command_prefix_registry::*;

pub mod mode_stack;
pub use mode_stack::*;

pub mod inner_chunk;
pub use inner_chunk::*;

pub mod chunks;
pub use chunks::*;

/// Initializes the editor's core state with essential components.
///
/// This function sets up the initial `State` object by registering various
/// critical components required for the editor's operation, such as window,
/// command handling, buffers, input, theming, and chunk management.
///
/// # Arguments
///
/// * `window`: The `ascii_forge` window instance that the editor will render to.
/// * `cmd_sender`: An `UnboundedSender` used to send commands to the editor's
///   main command processing loop.
///
/// # Returns
///
/// An initialized `State` object, ready to be used by the editor's main loop.
pub fn init_state(window: Window, cmd_sender: UnboundedSender<Box<dyn Command>>) -> State {
    let mut state = State::default();

    state
        // Editor's running status
        .state(Running(true))
        // Window management
        .state(WindowState(window))
        // Command sending channel
        .state(CommandSender(cmd_sender))
        // Buffer management, initialized with a scratch buffer
        .state({
            let mut buffers = Buffers::default();
            buffers
                .buffers
                .push(Arc::new(RwLock::new(TextBuffer::scratch())));
            buffers
        })
        // Input configuration and state
        .state(InputConfig::default())
        .state(InputState::default())
        // Theming
        .state(Theme::default())
        // Command palette specific state
        .state(CommandPaletteState::default())
        // Initial mode stack, starting with normal mode
        .state(ModeStack(vec!['n']))
        // Registry for all commands
        .state(CommandRegistry(vec![]))
        // Registry for command prefixes
        .state(CommandPrefixRegistry(vec![]))
        // Chunk management for drawing areas
        .state(Chunks::default())
        // Plugin specific configurations
        .state(PluginConfig(HashMap::default()));

    state
}
