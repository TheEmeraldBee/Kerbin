pub mod wrappers;
use std::{collections::HashMap, sync::Arc};

use ascii_forge::window::Window;
use tokio::sync::{RwLock, mpsc::UnboundedSender};
use uuid::Uuid;
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

pub mod registers;
pub use registers::*;

/// Initializes the editor's core state with essential components
pub fn init_state(
    window: Window,
    cmd_sender: UnboundedSender<Box<dyn Command>>,
    config_path: String,
    uuid: Uuid,
    server_ipc: ServerIpc,
) -> State {
    let mut state = State::default();

    let (log_state, log_sender) = LogState::new_with_channel();

    state
        .state(EventStorage::default())
        .state(Registers::default())
        .state(server_ipc)
        .state(QueryRegistry::default())
        .state(ConfigFolder(config_path))
        .state(SessionUuid(uuid))
        // Editor's running status
        .state(Running(true))
        .state(log_state)
        .state(log_sender)
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
