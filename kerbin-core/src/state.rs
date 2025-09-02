use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use ascii_forge::prelude::*;
use kerbin_plugin::Plugin;
use kerbin_state_machine::State;
use tokio::sync::mpsc::UnboundedSender;
use toml::Value;

use crate::{
    AsCommandInfo, Command, CommandInfo, CommandPaletteState, GrammarManager, InputConfig,
    InputState, TextBuffer, Theme, buffer::Buffers, rank,
};

pub struct Running(pub bool);

pub struct Plugins(pub Vec<Plugin>);

pub struct CommandRegistry(Vec<RegisteredCommandSet>);
impl CommandRegistry {
    pub fn register<T: AsCommandInfo + 'static>(&mut self) {
        self.0.push(RegisteredCommandSet {
            parser: Box::new(T::from_str),
            infos: T::infos(),
        })
    }

    pub fn split_command(input: &str) -> Vec<String> {
        shellwords::split(input).unwrap_or(vec![input.to_string()])
    }

    pub fn validate_command(
        &self,
        input: &str,
        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> bool {
        self.parse_command(
            Self::split_command(input),
            false,
            true,
            prefix_registry,
            modes,
        )
        .is_some()
    }

    pub fn get_command_suggestions(
        &self,
        input: &str,

        theme: &Theme,
    ) -> (Vec<Buffer>, Option<String>, Option<Buffer>) {
        let words = shellwords::split(input).unwrap_or(vec![input.to_string()]);

        if words.is_empty() {
            return (vec![], None, None);
        }

        let mut res = vec![];

        for registry in &self.0 {
            for info in &registry.infos {
                for valid_name in &info.valid_names {
                    let Some(rnk) = rank(&words[0], valid_name) else {
                        continue;
                    };

                    res.push((rnk, info, valid_name.to_string()));
                    break;
                }
            }
        }

        res.sort_by(|l, r| l.0.cmp(&r.0));

        let desc = res.first().and_then(|x| x.1.desc_buf(theme));

        let completion = if words.len() == 1 {
            res.first().map(|x| x.2.clone())
        } else {
            None
        };

        (
            res.iter()
                .enumerate()
                .map(|(i, x)| {
                    if i == 0 && completion.is_some() {
                        x.1.as_suggestion(true, theme)
                    } else {
                        x.1.as_suggestion(false, theme)
                    }
                })
                .collect(),
            completion,
            desc,
        )
    }

    pub fn parse_command(
        &self,
        mut words: Vec<String>,
        log_errors: bool,
        prefix_checked: bool,

        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> Option<Box<dyn Command>> {
        if !prefix_checked {
            for prefix in &prefix_registry.0 {
                if prefix.modes.iter().any(|x| modes.mode_on_stack(*x)) {
                    // Check that the cmd name is valid
                    let mut has_name = false;
                    if !prefix.list.is_empty() {
                        for infos in &self.0 {
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

        for registry in &self.0 {
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
}

pub struct CommandPrefixRegistry(pub Vec<CommandPrefix>);
impl CommandPrefixRegistry {
    pub fn register(&mut self, prefix: CommandPrefix) {
        self.0.push(prefix)
    }
}

pub struct ModeStack(pub Vec<char>);

pub struct PluginConfig(pub HashMap<String, Value>);

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

impl ModeStack {
    pub fn push_mode(&mut self, mode: char) {
        self.0.push(mode);
    }

    pub fn pop_mode(&mut self) -> Option<char> {
        if self.0.len() <= 1 {
            return None;
        }

        self.0.pop()
    }

    pub fn set_mode(&mut self, mode: char) {
        self.0.clear();
        self.0.push('n');
        // Since we already pushed normal mode.
        if mode == 'n' {
            return;
        }
        self.0.push(mode);
    }

    pub fn get_mode(&self) -> char {
        *self.0.last().unwrap()
    }

    pub fn mode_on_stack(&self, mode: char) -> bool {
        self.0.contains(&mode)
    }
}

pub type CommandSender = UnboundedSender<Box<dyn Command>>;

pub fn init_state(window: Window, cmd_sender: UnboundedSender<Box<dyn Command>>) -> State {
    let mut state = State::default();

    state
        .state(Running(true))
        .state(window)
        .state(Plugins(vec![]))
        .state(cmd_sender)
        .state({
            let mut buffers = Buffers::default();
            buffers
                .buffers
                .push(Arc::new(RwLock::new(TextBuffer::scratch())));
            buffers
        })
        .state(InputConfig::default())
        .state(InputState::default())
        .state(GrammarManager::default())
        .state(Theme::default())
        .state(CommandPaletteState::default())
        .state(ModeStack(vec!['n']))
        .state(CommandRegistry(vec![]))
        .state(CommandPrefixRegistry(vec![]))
        .state(PluginConfig(HashMap::default()));

    state
}
