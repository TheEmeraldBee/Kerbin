pub mod wrappers;
use std::{path::PathBuf, sync::Arc};

use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::{RwLock, mpsc::UnboundedSender};
use uuid::Uuid;
pub use wrappers::*;

pub mod command_registry;
pub use command_registry::*;

pub mod command_prefix_registry;
pub use command_prefix_registry::*;

pub mod command_interceptor_registry;
pub use command_interceptor_registry::*;

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
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
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
        .state(ConfigDir(PathBuf::from(format!("{config_path}/config"))))
        .state(CoreConfig::default())
        .state(PaletteState::default())
        .state(ConfigFolder(config_path))
        .state(SessionUuid(uuid))
        .state(Running(true))
        .state(log_state)
        .state(log_sender)
        .state(WindowState(terminal))
        .state(CrosstermEvents::default())
        .state(CommandSender(cmd_sender))
        .state({
            let mut buffers = Buffers::default();
            buffers
                .buffers
                .push(Arc::new(RwLock::new(TextBuffer::scratch())));
            buffers
        })
        .state(InputState::default())
        .state(Theme::default())
        .state(CommandPaletteState::default())
        .state(ModeStack(vec!['n']))
        .state(CommandRegistry(vec![]))
        .state(CommandPrefixRegistry(vec![]))
        .state(CommandInterceptorRegistry::new())
        .state(AutoPairs::default())
        .state(Chunks::default())
        .state(DebounceConfig::default())
        .state(StatuslineConfig::default())
        .state(LayoutConfig::default());

    state
}
