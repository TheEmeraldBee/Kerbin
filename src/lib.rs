#[macro_export]
macro_rules! term_print {
    ($format:expr $(,$args:expr)* $(,)?) => {
        crokey::crossterm::execute!(std::io::stdout(), crokey::crossterm::cursor::MoveTo(50, 10), crokey::crossterm::style::Print(format!($format, $($args)*))).unwrap()
    };
}

pub mod buffer;
pub use rune::sync::Arc;
use std::sync::{
    RwLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use ascii_forge::window::Window;
pub use buffer::*;

pub mod commands;
pub use commands::*;

pub mod input;
use crokey::Combiner;
use derive_more::{Deref, DerefMut};
pub use input::*;

pub mod key_check;

pub mod buffer_extensions;

pub mod plugin_manager;
pub use plugin_manager::*;

pub mod plugin_libs;

pub mod mode;
pub use mode::*;

pub mod command_palette;
pub use command_palette::*;

pub mod grammar;
pub use grammar::*;

pub mod highlight;
pub use highlight::*;

pub mod theme;
pub use theme::*;

pub mod shell;
pub use shell::*;

use tokio::sync::mpsc::UnboundedSender;

#[derive(rune::Any, Deref, DerefMut)]
pub struct AnyWindow(Window);

#[derive(rune::Any)]
pub struct AppState {
    pub running: AtomicBool,

    pub config: RwLock<PluginConfig>,

    pub window: RwLock<AnyWindow>,
    pub combiner: RwLock<Combiner>,

    pub commands: UnboundedSender<Box<dyn Command>>,
    pub command_success: AtomicBool,

    pub palette: RwLock<CommandPaletteState>,

    pub mode: AtomicU32,
    pub shell: RwLock<ShellLink>,
    pub buffers: RwLock<Buffers>,

    pub grammar: RwLock<GrammarManager>,
    pub theme: RwLock<Theme>,

    pub input_state: RwLock<InputState>,
    pub input: RwLock<InputConfig>,
}

impl AppState {
    pub fn new(
        window: Window,
        combiner: Combiner,
        command_sender: UnboundedSender<Box<dyn Command>>,
    ) -> Arc<Self> {
        Arc::try_new(Self {
            running: AtomicBool::new(true),

            config: RwLock::new(PluginConfig::default()),

            window: RwLock::new(AnyWindow(window)),
            combiner: RwLock::new(combiner),

            commands: command_sender,
            command_success: AtomicBool::new(false),

            palette: RwLock::new(CommandPaletteState::new()),

            mode: AtomicU32::new(u32::from('n')),

            shell: RwLock::new(ShellLink::new()),

            buffers: RwLock::new(Buffers::default()),

            grammar: RwLock::new(GrammarManager::default()),
            theme: RwLock::new(Theme::default()),

            input: RwLock::new(InputConfig::default()),
            input_state: RwLock::new(InputState::default()),
        })
        .unwrap()
    }

    pub fn set_mode(&self, mode: char) {
        self.mode.store(u32::from(mode), Ordering::Relaxed)
    }

    pub fn get_mode(&self) -> char {
        char::from_u32(self.mode.load(Ordering::Relaxed)).unwrap_or_default()
    }
}
