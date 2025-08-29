use std::{
    collections::HashMap,
    sync::{Arc, RwLock, atomic::AtomicBool},
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

#[derive(Debug)]
pub struct CommandPrefix {
    pub modes: Vec<char>,
    pub prefix_cmd: String,

    /// Whether to make the list var an include or exclude
    pub include: bool,
    /// Depending on include, this is an exclude or include list (default exclude)
    pub list: Vec<String>,
}

pub struct State {
    pub running: AtomicBool,

    pub mode_stack: RwLock<Vec<char>>,

    pub buffers: RwLock<Buffers>,

    pub window: RwLock<Window>,

    pub input_config: RwLock<InputConfig>,
    pub input_state: RwLock<InputState>,

    pub grammar: RwLock<GrammarManager>,
    pub theme: RwLock<Theme>,

    pub palette: RwLock<CommandPaletteState>,

    pub commands: UnboundedSender<Box<dyn Command>>,

    pub command_registry: RwLock<Vec<RegisteredCommandSet>>,
    pub prefix_registry: RwLock<Vec<CommandPrefix>>,

    pub plugin_config: RwLock<HashMap<String, Value>>,
}

impl State {
    pub fn new(window: Window, cmd_sender: UnboundedSender<Box<dyn Command>>) -> Self {
        Self {
            running: AtomicBool::new(true),

            mode_stack: RwLock::new(vec!['n']),

            buffers: RwLock::new(Buffers::default()),

            window: RwLock::new(window),

            input_config: RwLock::new(InputConfig::default()),
            input_state: RwLock::new(InputState::default()),

            grammar: RwLock::new(GrammarManager::new()),
            theme: RwLock::new(Theme::default()),

            palette: RwLock::new(CommandPaletteState::default()),

            commands: cmd_sender,

            command_registry: RwLock::new(Vec::new()),
            prefix_registry: RwLock::new(Vec::new()),

            plugin_config: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_command_suggestions(self: &Arc<Self>, input: &str) -> (Vec<Buffer>, Option<Buffer>) {
        let words = shellwords::split(input).unwrap_or(vec![input.to_string()]);

        if words.is_empty() {
            return (vec![], None);
        }

        let mut res = vec![];

        let theme = self.theme.read().unwrap();

        let mut desc = None;

        for registry in self.command_registry.read().unwrap().iter() {
            for info in &registry.infos {
                if info.valid_names.iter().any(|x| x.starts_with(&words[0])) {
                    if desc.is_none() {
                        desc = info.desc_buf(&theme)
                    }

                    res.push(info.as_suggestion(&theme))
                }
            }
        }

        (res, desc)
    }

    pub fn split_command(input: &str) -> Vec<String> {
        shellwords::split(input).unwrap_or(vec![input.to_string()])
    }

    pub fn parse_command(
        self: &Arc<Self>,
        mut words: Vec<String>,
        log_errors: bool,
        prefix_checked: bool,
    ) -> Option<Box<dyn Command>> {
        if !prefix_checked {
            for prefix in self.prefix_registry.read().unwrap().iter() {
                if prefix.modes.iter().any(|x| self.mode_on_stack(*x)) {
                    // Check that the cmd name is valid
                    let mut has_name = false;
                    if !prefix.list.is_empty() {
                        for infos in self.command_registry.read().unwrap().iter() {
                            if infos.infos.iter().any(|x| {
                                let matches_word0 = x.check_name(&words[0]);
                                let matches_prefix = prefix.list.iter().any(|l| x.check_name(l));
                                matches_word0 && matches_prefix
                            }) {
                                has_name = true;
                            }

                            if has_name {
                                break;
                            }
                        }
                    } else {
                        has_name = true
                    }

                    if prefix.include != has_name {
                        continue;
                    }

                    // Prefix the command with the resulting prefix split
                    let mut new_words = Self::split_command(&prefix.prefix_cmd);
                    new_words.append(&mut words);

                    words = new_words;
                }
            }
        }

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
        self.parse_command(Self::split_command(input), false, true)
            .is_some()
    }

    pub fn call_command(self: &Arc<Self>, input: &str) -> bool {
        match self.parse_command(Self::split_command(input), true, false) {
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

    pub fn register_command_prefix(&self, prefix: CommandPrefix) {
        self.prefix_registry.write().unwrap().push(prefix);
    }

    pub fn pop_mode(&self) -> char {
        let mut modes = self.mode_stack.write().unwrap();
        if modes.len() > 1 {
            modes.pop().unwrap()
        } else {
            // Can't pop the last mode on the stack, but n is always the base
            'n'
        }
    }

    pub fn push_mode(&self, mode: char) {
        self.mode_stack.write().unwrap().push(mode);
    }

    pub fn set_mode(&self, mode: char) {
        let mut modes = self.mode_stack.write().unwrap();
        modes.clear();
        modes.push('n');
        // Since we already pushed normal mode.
        if mode == 'n' {
            return;
        }
        modes.push(mode);
    }

    pub fn get_mode(&self) -> char {
        *self.mode_stack.read().unwrap().last().unwrap()
    }

    pub fn mode_on_stack(&self, mode: char) -> bool {
        self.mode_stack.read().unwrap().contains(&mode)
    }
}
