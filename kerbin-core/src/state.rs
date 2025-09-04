use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock},
};

use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};
use kerbin_macros::State;
use kerbin_plugin::Plugin;
use kerbin_state_machine::State;
use kerbin_state_machine::storage::*;
use tokio::sync::mpsc::UnboundedSender;
use toml::Value;

use crate::{
    AsCommandInfo, Command, CommandInfo, CommandPaletteState, GrammarManager, InputConfig,
    InputState, Rect, TextBuffer, Theme, buffer::Buffers, rank,
};

#[derive(State)]
pub struct Running(pub bool);

#[derive(State)]
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

#[derive(State)]
pub struct CommandPrefixRegistry(pub Vec<CommandPrefix>);
impl CommandPrefixRegistry {
    pub fn register(&mut self, prefix: CommandPrefix) {
        self.0.push(prefix)
    }
}

#[derive(State)]
pub struct ModeStack(pub Vec<char>);

#[derive(State)]
pub struct PluginConfig(pub HashMap<String, Value>);

#[derive(State)]
pub struct CommandSender(UnboundedSender<Box<dyn Command>>);

#[derive(State)]
pub struct InnerChunk {
    buffer: Buffer,
    cursor: Option<(usize, Vec2, SetCursorStyle)>,
}

impl Deref for InnerChunk {
    type Target = Buffer;
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for InnerChunk {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl InnerChunk {
    pub fn new(buf: Buffer) -> Self {
        Self {
            buffer: buf,
            cursor: None,
        }
    }

    pub fn remove_cursor(&mut self) {
        self.cursor = None;
    }

    pub fn set_cursor(&mut self, priority: usize, pos: Vec2, style: SetCursorStyle) {
        self.cursor = Some((priority, pos, style))
    }

    pub fn cursor_pos(&self) -> Option<Vec2> {
        self.cursor.as_ref().map(|x| x.1)
    }

    pub fn cursor_set(&self) -> bool {
        self.cursor.is_some()
    }

    pub fn get_full_cursor(&self) -> &Option<(usize, Vec2, SetCursorStyle)> {
        &self.cursor
    }
}

#[derive(State, Default)]
pub struct Chunks {
    pub buffers: Vec<Vec<(Vec2, Arc<RwLock<InnerChunk>>)>>,
    chunk_idx_map: HashMap<String, (usize, usize)>,
}

impl Chunks {
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.chunk_idx_map.clear();
    }

    pub fn register_chunk<C: StateName + StaticState>(&mut self, z_index: usize, rect: Rect) {
        let size = (rect.width, rect.height);
        let pos = (rect.x, rect.y);

        if self.buffers.len() <= z_index {
            self.buffers.resize(z_index + 1, Vec::default());
        }

        let coords = self
            .chunk_idx_map
            .entry(C::static_name())
            .or_insert((z_index, self.buffers[z_index].len()));

        if self.buffers[z_index].len() == coords.1 {
            self.buffers[z_index].push((
                pos.into(),
                Arc::new(RwLock::new(InnerChunk::new(Buffer::new(size)))),
            ));
        } else {
            self.buffers[z_index][coords.1] = (
                pos.into(),
                Arc::new(RwLock::new(InnerChunk::new(Buffer::new(size)))),
            );
        }
    }

    pub fn get_chunk<C: StateName + StaticState>(&self) -> Option<Arc<RwLock<InnerChunk>>> {
        let id = C::static_name();

        let (ia, ib) = self.chunk_idx_map.get(&id)?;

        Some(self.buffers[*ia][*ib].1.clone())
    }
}

impl Deref for CommandSender {
    type Target = UnboundedSender<Box<dyn Command>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CommandSender {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(State)]
pub struct WindowState(Window);

impl Deref for WindowState {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for WindowState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

type CommandFn = Box<dyn Fn(&[String]) -> Option<Result<Box<dyn Command>, String>> + Send + Sync>;

pub struct RegisteredCommandSet {
    pub parser: CommandFn,
    pub infos: Vec<CommandInfo>,
}

#[derive(Debug, State)]
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

pub fn init_state(window: Window, cmd_sender: UnboundedSender<Box<dyn Command>>) -> State {
    let mut state = State::default();

    state
        .state(Running(true))
        .state(WindowState(window))
        .state(CommandSender(cmd_sender))
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
        .state(Chunks::default())
        .state(PluginConfig(HashMap::default()));

    state
}
