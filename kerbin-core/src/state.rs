use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use ascii_forge::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    AsCommandInfo, Command, CommandInfo, GrammarManager, InputConfig, InputState, Theme,
    buffer::Buffers,
};

type CommandFn = Box<dyn Fn(&[String]) -> Option<Result<Box<dyn Command>, String>> + Send + Sync>;

pub struct RegisteredCommandSet {
    pub parser: CommandFn,
    pub infos: Vec<CommandInfo>,
}

pub struct State {
    pub running: AtomicBool,

    mode: AtomicU32,

    pub buffers: RwLock<Buffers>,

    pub window: RwLock<Window>,

    pub input_config: RwLock<InputConfig>,
    pub input_state: RwLock<InputState>,

    pub grammar: RwLock<GrammarManager>,
    pub theme: RwLock<Theme>,

    pub commands: UnboundedSender<Box<dyn Command>>,

    pub command_registry: RwLock<Vec<RegisteredCommandSet>>,
}

impl State {
    pub fn new(window: Window, cmd_sender: UnboundedSender<Box<dyn Command>>) -> Self {
        Self {
            running: AtomicBool::new(true),

            mode: AtomicU32::new(u32::from('n')),

            buffers: RwLock::new(Buffers::default()),

            window: RwLock::new(window),

            input_config: RwLock::new(InputConfig::default()),
            input_state: RwLock::new(InputState::default()),

            grammar: RwLock::new(GrammarManager::new()),
            theme: RwLock::new(Theme::default()),

            commands: cmd_sender,

            command_registry: RwLock::new(Vec::new()),
        }
    }

    pub fn call_command(self: &Arc<Self>, command: &str) -> bool {
        let words = shellwords::split(command).unwrap();

        for registry in self.command_registry.read().unwrap().iter() {
            if let Some(cmd) = (registry.parser)(&words) {
                match cmd {
                    Ok(t) => return t.apply(self.clone()),
                    Err(e) => {
                        tracing::error!("Failed to parse command due to: {e:?}");
                        return false;
                    }
                }
            }
        }
        false
    }

    pub fn register_command_deserializer<T: AsCommandInfo + 'static>(&self) {
        self.command_registry
            .write()
            .unwrap()
            .push(RegisteredCommandSet {
                parser: Box::new(T::from_str),
                infos: T::infos(),
            });
    }

    pub fn set_mode(&self, mode: char) {
        self.mode.store(u32::from(mode), Ordering::Relaxed);
    }

    pub fn get_mode(&self) -> char {
        char::from_u32(self.mode.load(Ordering::Relaxed)).unwrap_or_default()
    }
}
