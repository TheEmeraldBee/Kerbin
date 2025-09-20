use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock},
};

use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};
use kerbin_macros::State;
use kerbin_state_machine::State;
use kerbin_state_machine::storage::*;
use tokio::sync::mpsc::UnboundedSender;
use toml::Value;

use crate::{
    AsCommandInfo, Command, CommandInfo, CommandPaletteState, InputConfig, InputState, Rect,
    TextBuffer, Theme, buffer::Buffers, rank,
};

/// Primary state marking whether the core editor is running at this moment.
///
/// When set to false, the editor will exit at the end of the current frame.
#[derive(State)]
pub struct Running(pub bool);

/// State used for storing registered commands within the system itself.
///
/// Also used for parsing and validating commands.
#[derive(State)]
pub struct CommandRegistry(Vec<RegisteredCommandSet>);

impl CommandRegistry {
    /// Registers the given command type within the editor using its implementation.
    ///
    /// The command type must implement the `AsCommandInfo` trait to provide command metadata.
    ///
    /// # Type Parameters
    ///
    /// * `T`: The command type that implements `AsCommandInfo`.
    pub fn register<T: AsCommandInfo + 'static>(&mut self) {
        self.0.push(RegisteredCommandSet {
            parser: Box::new(T::from_str),
            infos: T::infos(),
        })
    }

    /// Splits the command's string using `shellwords` in order to handle commands
    /// closer to how a shell does. Command parser and validator do this automatically.
    ///
    /// This function handles quoted strings and other shell-like parsing rules.
    ///
    /// # Arguments
    ///
    /// * `input`: The command string to split.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the split words of the command. Returns a vector
    /// with the original input as a single string if splitting fails.
    pub fn split_command(input: &str) -> Vec<String> {
        shellwords::split(input).unwrap_or(vec![input.to_string()])
    }

    /// Given the input, will return whether the input would be considered a valid command.
    ///
    /// This method attempts to parse the command without logging errors, checking against
    /// registered commands and applying command prefixes if applicable.
    ///
    /// # Arguments
    ///
    /// * `input`: The input string to validate.
    /// * `prefix_registry`: A reference to the `CommandPrefixRegistry` for checking against
    ///                      registered command prefixes.
    /// * `modes`: A reference to the `ModeStack` to determine active modes for
    ///            mode-specific command validation.
    ///
    /// # Returns
    ///
    /// `true` if the input can be successfully parsed into a command, `false` otherwise.
    pub fn validate_command(
        &self,
        input: &str,
        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> bool {
        self.parse_command(
            Self::split_command(input),
            false, // Do not log errors during validation
            true,  // Indicate that prefix checking should happen (or has happened)
            prefix_registry,
            modes,
        )
        .is_some()
    }

    /// Retrieves the buffers used for handling the rendering of
    /// command palette within the core engine.
    /// Buffers are used so theming can be stored directly.
    ///
    /// This method takes an input string, attempts to find matching commands, ranks them
    /// by relevance, and generates `Buffer` representations for display, along with
    /// an optional completion string and a description buffer for the top suggestion.
    ///
    /// # Arguments
    ///
    /// * `input`: The current input string entered into the command palette.
    /// * `theme`: The current `Theme` to apply styling to the suggestion buffers.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - `Vec<Buffer>`: A vector of `Buffer` instances, each representing a command suggestion.
    ///   The top suggestion is styled differently if a completion is available.
    /// - `Option<String>`: An optional string representing the auto-completion for the
    ///   first word of the input if there's a clear top suggestion.
    /// - `Option<Buffer>`: An optional `Buffer` containing the detailed description
    ///   of the top command suggestion.
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

    /// Parses a given list of command words into a `Box<dyn Command>`.
    ///
    /// This is the core method for converting user input into executable commands. It handles
    /// command prefixes based on the current mode stack and attempts to parse the words
    /// against all registered command parsers.
    ///
    /// # Arguments
    ///
    /// * `words`: A `Vec<String>` representing the command and its arguments. This vector
    ///            might be modified if command prefixes are applied.
    /// * `log_errors`: If `true`, parsing errors (returned by command parsers) will be
    ///                 logged using `tracing::error!`.
    /// * `prefix_checked`: If `true`, the command prefix application logic will be skipped.
    ///                     This is useful if the command has already been pre-processed.
    /// * `prefix_registry`: A reference to the `CommandPrefixRegistry` used to find and
    ///                      apply command prefixes based on the current mode.
    /// * `modes`: A reference to the `ModeStack` to determine the currently active modes,
    ///            which influence which command prefixes are applied.
    ///
    /// # Returns
    ///
    /// An `Option<Box<dyn Command>>` containing the parsed and boxed command if successful.
    /// Returns `None` if the input cannot be parsed into a valid command, or if an error
    /// occurs during parsing and `log_errors` is true.
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

/// State used for storing registered command prefixes.
///
/// Contains a vector of `CommandPrefix` configurations.
#[derive(State)]
pub struct CommandPrefixRegistry(pub Vec<CommandPrefix>);

impl CommandPrefixRegistry {
    /// Registers a new command prefix configuration.
    ///
    /// This adds a `CommandPrefix` to the registry, which can then be used
    /// by the `CommandRegistry` to modify user input based on active modes.
    ///
    /// # Arguments
    ///
    /// * `prefix`: The `CommandPrefix` to register.
    pub fn register(&mut self, prefix: CommandPrefix) {
        self.0.push(prefix)
    }
}

/// State representing the current mode stack of the editor.
///
/// The mode stack determines the current operational context of the editor,
/// influencing keybindings, command prefixes, and other behaviors.
/// The bottom of the stack is typically 'n' for normal mode.
#[derive(State)]
pub struct ModeStack(pub Vec<char>);

/// State storing plugin configuration as a map of plugin names to TOML `Value`.
///
/// This allows plugins to store and retrieve their configurations dynamically.
#[derive(State)]
pub struct PluginConfig(pub HashMap<String, Value>);

/// State for sending commands through an unbounded MPSC sender.
///
/// This provides a mechanism for different parts of the application to
/// enqueue commands to be processed asynchronously by the main event loop.
#[derive(State)]
pub struct CommandSender(UnboundedSender<Box<dyn Command>>);

/// An internal chunk representing a buffer and an optional cursor.
///
/// This struct is used by the `Chunks` state to manage individual drawing areas
/// within the editor, each potentially having its own cursor.
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
    /// Creates a new `InnerChunk` with the given buffer and no cursor initially.
    ///
    /// # Arguments
    ///
    /// * `buf`: The `Buffer` to wrap.
    pub fn new(buf: Buffer) -> Self {
        Self {
            buffer: buf,
            cursor: None,
        }
    }

    /// Removes the cursor from this chunk, if one was set.
    pub fn remove_cursor(&mut self) {
        self.cursor = None;
    }

    /// Sets the cursor for this chunk with a specified priority, position, and style.
    ///
    /// The priority can be used to resolve conflicts if multiple chunks attempt to
    /// set the cursor simultaneously.
    ///
    /// # Arguments
    ///
    /// * `priority`: The priority level of this cursor. Higher values typically mean higher priority.
    /// * `pos`: The `Vec2` coordinates of the cursor within the chunk's buffer.
    /// * `style`: The `SetCursorStyle` to apply to the cursor.
    pub fn set_cursor(&mut self, priority: usize, pos: Vec2, style: SetCursorStyle) {
        self.cursor = Some((priority, pos, style))
    }

    /// Returns the position of the cursor if set.
    ///
    /// # Returns
    ///
    /// An `Option<Vec2>` representing the cursor's position within the chunk's buffer,
    /// or `None` if no cursor is set.
    pub fn cursor_pos(&self) -> Option<Vec2> {
        self.cursor.as_ref().map(|x| x.1)
    }

    /// Returns `true` if a cursor is set for this chunk, `false` otherwise.
    pub fn cursor_set(&self) -> bool {
        self.cursor.is_some()
    }

    /// Returns a reference to the full cursor information.
    ///
    /// This includes priority, position, and style, if a cursor is set.
    ///
    /// # Returns
    ///
    /// A reference to `Option<(usize, Vec2, SetCursorStyle)>`.
    pub fn get_full_cursor(&self) -> &Option<(usize, Vec2, SetCursorStyle)> {
        &self.cursor
    }
}

/// State managing and organizing drawing chunks (buffers).
///
/// `Chunks` provides a way to register and retrieve `InnerChunk` instances,
/// allowing different UI components to manage their own drawing areas.
/// Chunks are organized by a Z-index for layering.
#[derive(State, Default)]
pub struct Chunks {
    /// A vector of vectors, where the outer vector represents Z-layers
    /// and the inner vector holds `(position, InnerChunk)` pairs for that layer.
    pub buffers: Vec<Vec<(Vec2, Arc<RwLock<InnerChunk>>)>>,
    /// A map from state name (identifier for a chunk) to its `(z_index, inner_vec_index)` coordinates.
    chunk_idx_map: HashMap<String, (usize, usize)>,
}

impl Chunks {
    /// Clears all registered chunks and their associated buffers.
    ///
    /// This effectively resets the entire chunk management system.
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.chunk_idx_map.clear();
    }

    /// Registers a new chunk for drawing, identified by its state name.
    ///
    /// If a chunk with the given `C::static_name()` is already registered at the
    /// specified `z_index`, its existing entry might be updated. Otherwise, a new
    /// chunk is created and added. The size of the `InnerChunk`'s buffer is derived
    /// from the `rect`.
    ///
    /// # Type Parameters
    ///
    /// * `C`: The state type that implements `StateName` and `StaticState`. This type's
    ///        `static_name()` method provides a unique identifier for the chunk.
    ///
    /// # Arguments
    ///
    /// * `z_index`: The Z-index (layer) at which to draw this chunk. Higher indices
    ///              are drawn on top of lower indices.
    /// * `rect`: The `Rect` defining the position and size (width and height) of the chunk.
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
            // Add new chunk if not already present at this exact inner index
            self.buffers[z_index].push((
                pos.into(),
                Arc::new(RwLock::new(InnerChunk::new(Buffer::new(size)))),
            ));
        } else {
            // Otherwise, update existing chunk (e.g., if its dimensions changed)
            self.buffers[z_index][coords.1] = (
                pos.into(),
                Arc::new(RwLock::new(InnerChunk::new(Buffer::new(size)))),
            );
        }
    }

    /// Retrieves a registered chunk by its state name.
    ///
    /// This method allows access to the `InnerChunk` associated with a specific
    /// UI component or state, identified by its static name.
    ///
    /// # Type Parameters
    ///
    /// * `C`: The state type that implements `StateName` and `StaticState`, used to
    ///        identify the chunk via `C::static_name()`.
    ///
    /// # Returns
    ///
    /// An `Option<Arc<RwLock<InnerChunk>>>` containing a thread-safe reference to the
    /// chunk if found, or `None` if no chunk is registered under that name.
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

/// State wrapper around the `ascii_forge` window.
///
/// This allows the main `ascii_forge` window to be managed as part of the editor's state,
/// enabling other systems to access and manipulate it.
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

/// Type alias for a command parsing function.
///
/// This defines the signature required for functions that can parse a slice of
/// strings (command words) into an `Option` of `Result` containing a boxed `Command`.
type CommandFn = Box<dyn Fn(&[String]) -> Option<Result<Box<dyn Command>, String>> + Send + Sync>;

/// Represents a set of registered commands, including its parser and command information.
///
/// Each `RegisteredCommandSet` groups a parser function with the metadata
/// (`CommandInfo`) for the commands it can parse.
pub struct RegisteredCommandSet {
    /// The function responsible for parsing a list of string arguments into a command.
    pub parser: CommandFn,
    /// A vector of `CommandInfo` structs, providing metadata for the commands handled by this parser.
    pub infos: Vec<CommandInfo>,
}

/// Represents a command prefix configuration.
///
/// Command prefixes allow automatically prepending specific commands or arguments
/// to user input based on the active editor mode.
#[derive(Debug, State)]
pub struct CommandPrefix {
    /// A list of character codes representing the modes in which this prefix should be active.
    /// If any of these modes are on the `ModeStack`, the prefix logic will be applied.
    pub modes: Vec<char>,
    /// The command string that will be prepended to the user's input. This string
    /// is split into words using `shellwords::split` before prepending.
    pub prefix_cmd: String,

    /// A boolean indicating whether the `list` acts as an `include` filter (`true`)
    /// or an `exclude` filter (`false`, default) for command names.
    pub include: bool,
    /// Depending on the `include` flag, this is either an inclusion list (only commands
    /// in this list are prefixed) or an exclusion list (commands in this list are NOT prefixed).
    pub list: Vec<String>,
}

impl ModeStack {
    /// Pushes a new mode onto the mode stack.
    ///
    /// The newly pushed mode becomes the current active mode.
    ///
    /// # Arguments
    ///
    /// * `mode`: The character representing the mode to push (e.g., 'i' for insert mode).
    pub fn push_mode(&mut self, mode: char) {
        self.0.push(mode);
    }

    /// Pops the top mode from the mode stack.
    ///
    /// If only one mode remains (typically 'n' for normal mode), it cannot be popped
    /// to ensure there's always an active mode.
    ///
    /// # Returns
    ///
    /// An `Option<char>` containing the popped mode, or `None` if only one mode remains
    /// and thus cannot be popped.
    pub fn pop_mode(&mut self) -> Option<char> {
        if self.0.len() <= 1 {
            return None;
        }

        self.0.pop()
    }

    /// Sets the current mode, clearing all other modes and ensuring 'n' (normal mode)
    /// is at the bottom of the stack, followed by the specified mode if it's not 'n'.
    ///
    /// This effectively switches the editor to a new, single-active mode.
    ///
    /// # Arguments
    ///
    /// * `mode`: The character representing the mode to set as the current active mode.
    pub fn set_mode(&mut self, mode: char) {
        self.0.clear();
        self.0.push('n');
        // Since we already pushed normal mode.
        if mode == 'n' {
            return;
        }
        self.0.push(mode);
    }

    /// Returns the current active mode (the top-most mode on the stack).
    ///
    /// # Panics
    ///
    /// Panics if the mode stack is empty. This scenario should ideally be prevented
    /// by always ensuring 'n' mode is present.
    ///
    /// # Returns
    ///
    /// The character representing the current active mode.
    pub fn get_mode(&self) -> char {
        *self.0.last().unwrap()
    }

    /// Checks if a given mode is currently present anywhere on the mode stack.
    ///
    /// This is useful for determining if the editor is in a specific mode,
    /// even if it's not the top-most (current) mode.
    ///
    /// # Arguments
    ///
    /// * `mode`: The character representing the mode to check for.
    ///
    /// # Returns
    ///
    /// `true` if the mode is found on the stack, `false` otherwise.
    pub fn mode_on_stack(&self, mode: char) -> bool {
        self.0.contains(&mode)
    }
}

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
///                 main command processing loop.
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
