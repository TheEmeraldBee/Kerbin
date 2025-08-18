use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use ascii_forge::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{Command, CommandFromStr, InputConfig, InputState, buffer::Buffers};

pub struct State {
    pub running: AtomicBool,

    mode: AtomicU32,

    pub buffers: RwLock<Buffers>,

    pub window: RwLock<Window>,

    pub input_config: RwLock<InputConfig>,
    pub input_state: RwLock<InputState>,

    pub commands: UnboundedSender<Box<dyn Command>>,

    pub deser_command_registry:
        RwLock<Vec<Box<dyn Fn(&[String]) -> Option<Box<dyn Command>> + Send + Sync>>>,
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

            commands: cmd_sender,

            deser_command_registry: RwLock::new(Vec::new()),
        }
    }

    pub fn call_command(self: &Arc<Self>, command: &str) -> bool {
        let words = shellwords::split(command).unwrap();

        for registry in self.deser_command_registry.read().unwrap().iter() {
            if let Some(cmd) = registry(&words) {
                return cmd.apply(self.clone());
            }
        }
        false
    }

    pub fn register_command_deserializer<T: CommandFromStr + 'static>(&self) {
        self.deser_command_registry
            .write()
            .unwrap()
            .push(Box::new(T::from_str));
    }

    pub fn set_mode(&self, mode: char) {
        self.mode.store(u32::from(mode), Ordering::Relaxed);
    }

    pub fn get_mode(&self) -> char {
        char::from_u32(self.mode.load(Ordering::Relaxed)).unwrap_or_default()
    }
}
