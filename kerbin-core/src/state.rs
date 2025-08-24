use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use ascii_forge::prelude::*;
use tokio::sync::mpsc::UnboundedSender;
use toml::Value;

use crate::{
    AsCommandInfo, Command, CommandInfo, CommandPaletteState, GrammarManager, InputConfig,
    InputState, Theme, buffer::Buffers,
};

type CommandFn = Box<dyn Fn(&[String]) -> Option<Result<Box<dyn Command>, String>> + Send + Sync>;

pub struct RegisteredCommandSet {
    pub parser: CommandFn,
    pub infos: Vec<CommandInfo>,
}

#[macro_export]
macro_rules! cmd {
    ($state:expr, $func:path, $($arg:expr),*) => {
        $func($state.clone(), $($arg),*)
    };
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

    pub palette: RwLock<CommandPaletteState>,

    pub commands: UnboundedSender<Box<dyn Command>>,

    pub command_registry: RwLock<Vec<RegisteredCommandSet>>,

    pub plugin_config: RwLock<HashMap<String, Value>>,
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

            palette: RwLock::new(CommandPaletteState::default()),

            commands: cmd_sender,

            command_registry: RwLock::new(Vec::new()),

            plugin_config: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_command_suggestions(self: &Arc<Self>, input: &str) -> Vec<Buffer> {
        let words = shellwords::split(input).unwrap_or(vec![input.to_string()]);

        if words.is_empty() {
            return vec![];
        }

        let mut res = vec![];

        let theme = self.theme.read().unwrap();

        for registry in self.command_registry.read().unwrap().iter() {
            for info in &registry.infos {
                if info.valid_names.iter().any(|x| x.starts_with(&words[0])) {
                    res.push(info.as_suggestion(&theme))
                }
            }
        }

        res
    }

    pub fn parse_command(
        self: &Arc<Self>,
        input: &str,
        log_errors: bool,
    ) -> Option<Box<dyn Command>> {
        let words = shellwords::split(input).unwrap_or(vec![input.to_string()]);

        if words.is_empty() {
            return None;
        }

        for registry in self.command_registry.read().unwrap().iter() {
            if let Some(cmd) = (registry.parser)(&words) {
                match cmd {
                    Ok(t) => return Some(t),
                    Err(e) => {
                        if log_errors {
                            tracing::error!("Failed to parse command due to: {e:?}");
                        }
                        return None;
                    }
                }
            }
        }
        None
    }

    pub fn validate_command(self: &Arc<Self>, input: &str) -> bool {
        self.parse_command(input, false).is_some()
    }

    pub fn call_command(self: &Arc<Self>, input: &str) -> bool {
        match self.parse_command(input, true) {
            Some(t) => t.apply(self.clone()),
            None => false,
        }
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
