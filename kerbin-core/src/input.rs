use std::collections::VecDeque;

use ascii_forge::{prelude::*, widgets::border::Border};
use kerbin_macros::State;
use kerbin_state_machine::storage::*;
use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};

use crate::*;

/// Represents the possible outcomes when processing a single step of an input sequence.
pub enum InputResult {
    /// The input step did not match the expected key, or the mode was invalid.
    Failed,
    /// The input step matched, and more steps are expected to complete the sequence.
    Step,
    /// The input step matched, and the entire sequence is now complete.
    Complete,
}

/// Represents the event or action triggered when an `Input` sequence is completed.
pub enum InputEvent {
    /// A list of command strings to be executed.
    Commands(Vec<String>),
}

/// Represents a single input binding, mapping a key sequence to an action.
pub struct Input {
    /// A list of modes in which this input binding is valid. If empty, it's valid in all modes
    /// not listed in `invalid_modes`.
    pub valid_modes: Vec<char>,
    /// A list of modes in which this input binding is explicitly invalid.
    pub invalid_modes: Vec<char>,
    /// The sequence of key presses (modifiers + key code) that triggers this input.
    pub key_sequence: Vec<(KeyModifiers, KeyCode)>,
    /// The event that will be triggered when this input sequence is completed.
    pub event: InputEvent,

    /// A short description of what this input binding does, used in help menus.
    pub desc: String,
}

/// A utility function to check if two `KeyCode`s are equal, handling case-insensitive
/// comparison for `KeyCode::Char` variants.
///
/// This is required because `crossterm`'s `KeyCode::Char` does not automatically
/// handle case-insensitivity in its `PartialEq` implementation.
///
/// # Arguments
///
/// * `a`: The first `KeyCode`.
/// * `b`: The second `KeyCode`.
///
/// # Returns
///
/// `true` if the key codes are considered equal (case-insensitive for `Char`), `false` otherwise.
pub fn check_key_code(a: KeyCode, b: KeyCode) -> bool {
    match (a, b) {
        (KeyCode::Char(aa), KeyCode::Char(bb)) => aa.eq_ignore_ascii_case(&bb),
        _ => a == b,
    }
}

impl Input {
    /// Processes a single key event against the current step of this input binding.
    ///
    /// Checks if the provided mode is valid for this binding and if the current key event
    /// matches the expected key at `step` in the `key_sequence`.
    ///
    /// # Arguments
    ///
    /// * `window`: A reference to the `Window` resource to access recent key events.
    /// * `mode`: The current active editor mode.
    /// * `step`: The current step in the `key_sequence` being matched.
    ///
    /// # Returns
    ///
    /// An `InputResult` indicating whether the step failed, advanced, or completed the sequence.
    pub fn step(&self, window: &Window, mode: char, step: usize) -> InputResult {
        if (!self.valid_modes.is_empty() && !self.valid_modes.contains(&mode))
            || self.invalid_modes.contains(&mode)
        {
            return InputResult::Failed;
        }

        let seq = &self.key_sequence[step];
        if event!(window, Event::Key(k) => k.modifiers == seq.0 && check_key_code(k.code, seq.1)) {
            if self.key_sequence.len() == step + 1 {
                InputResult::Complete
            } else {
                InputResult::Step
            }
        } else {
            InputResult::Failed
        }
    }

    /// Generates a string representation of the remaining key sequence for display.
    ///
    /// This is used, for example, in the help menu to show users what keys they still need to press.
    ///
    /// # Arguments
    ///
    /// * `skip`: The number of initial key sequence steps to skip (e.g., how many steps have already been matched).
    ///
    /// # Returns
    ///
    /// A `String` representing the remaining key sequence, with modifiers and key codes
    /// formatted in a readable way (e.g., "ctrl-x y").
    pub fn sequence_str(&self, skip: usize) -> String {
        self.key_sequence
            .iter()
            .skip(skip)
            .map(|x| {
                format!(
                    "{}{}{}",
                    x.0.to_string().to_lowercase(),
                    if x.0.to_string().is_empty() {
                        "".to_string()
                    } else {
                        "-".to_string()
                    },
                    x.1.to_string().to_lowercase(),
                )
            })
            .reduce(|a, b| format!("{} {}", a, b))
            .unwrap_or_default()
    }
}

/// Stores all registered input bindings in the editor.
///
/// Input bindings are processed in reverse order of registration (most recently
/// registered inputs are checked first).
#[derive(Default, State)]
pub struct InputConfig {
    /// A double-ended queue holding all registered `Input` bindings.
    inputs: VecDeque<Input>,
}

impl InputConfig {
    /// Registers a new input binding, pushing it to the front of the queue.
    ///
    /// This means newly registered inputs will take precedence over older ones
    /// if their key sequences overlap.
    ///
    /// # Arguments
    ///
    /// * `input`: The `Input` binding to register.
    pub fn register_input(&mut self, input: Input) {
        self.inputs.push_front(input)
    }
}

/// Stores the current state of input processing, including repeat counts and active multi-key sequences.
#[derive(Default, State)]
pub struct InputState {
    /// A string representing a numeric repeat count entered by the user (e.g., "5" for `5dd`).
    pub(crate) repeat_count: String,
    /// A vector of currently active multi-key input sequences.
    /// Each tuple `(input_idx, step)` indicates which `Input` binding from `InputConfig`
    /// is active (`input_idx`) and which `step` in its `key_sequence` has been matched so far.
    pub(crate) active_inputs: Vec<(usize, usize)>,
}

/// Registers the help menu chunk in the UI if there are active input sequences.
///
/// This system dynamically creates a dedicated drawing area (`HelpChunk`) in the
/// bottom-right corner of the window to display active input sequences.
///
/// # Arguments
///
/// * `window`: `Res<WindowState>` providing access to window dimensions.
/// * `chunks`: `ResMut<Chunks>` for registering new drawing chunks.
/// * `input`: `Res<InputState>` to check if any input sequences are currently active.
pub async fn register_help_menu_chunk(
    window: Res<WindowState>,
    chunks: ResMut<Chunks>,
    input: Res<InputState>,
) {
    get!(input);
    if input.active_inputs.is_empty() {
        return;
    }

    get!(mut chunks, window);

    // Place a layout in the bottom right corner
    let rect = Layout::new()
        .row(flexible(), vec![flexible()])
        .row(
            // Ensure space for all active inputs (+2 for border)
            fixed(input.active_inputs.len() as u16 + 2),
            vec![flexible(), percent(20.0), fixed(1)],
        )
        .row(fixed(1), vec![flexible()])
        .calculate(window.size())
        .unwrap()[1][1];

    // This must render above the buffer, or the 0 z-index
    chunks.register_chunk::<HelpChunk>(1, rect);
}

/// Renders the help menu, displaying currently active input sequences.
///
/// This system draws the border and the descriptions of the key sequences
/// that are partially matched, providing user feedback for multi-key bindings.
///
/// # Arguments
///
/// * `chunk`: `Chunk<HelpChunk>` providing mutable access to the help menu's drawing buffer.
/// * `input`: `Res<InputState>` to get the list of active input sequences.
/// * `input_config`: `Res<InputConfig>` to retrieve the full `Input` binding details.
pub async fn render_help_menu(
    chunk: Chunk<HelpChunk>,
    input: Res<InputState>,
    input_config: Res<InputConfig>,
) {
    get!(input);
    if input.active_inputs.is_empty() {
        return;
    }

    let mut chunk = &mut chunk.get().await.unwrap();
    let input_config = input_config.get().await;

    let border = Border::square(chunk.size().x, chunk.size().y);

    render!(&mut chunk, (0, 0) => [border]);

    // Render up to the chunk's height (-2 on size for border)
    for i in 0..input.active_inputs.len().min(chunk.size().y as usize - 2) {
        let active = input.active_inputs[i];
        let binding = &input_config.inputs[active.0];
        render!(&mut chunk, vec2(1, 1 + i as u16) => [ binding.sequence_str(active.1), " - ", binding.desc ]);
    }
}

/// Handles incoming key events, processes input sequences, and dispatches commands.
///
/// This is the central input processing system of the editor. It:
/// 1. Handles character input in insert mode.
/// 2. Accumulates numeric repeat counts.
/// 3. Matches active multi-key sequences.
/// 4. Dispatches commands when an input sequence is completed.
/// 5. Initiates new input sequences.
///
/// # Arguments
///
/// * `window`: `Res<WindowState>` to access key events.
/// * `input`: `ResMut<InputState>` for mutable access to repeat count and active inputs.
/// * `input_config`: `Res<InputConfig>` for registered input bindings.
/// * `modes`: `Res<ModeStack>` to get the current editor mode.
/// * `command_registry`: `Res<CommandRegistry>` for parsing command strings.
/// * `prefix_registry`: `Res<CommandPrefixRegistry>` for applying command prefixes.
/// * `command_sender`: `ResMut<CommandSender>` for dispatching executed commands.
pub async fn handle_inputs(
    window: Res<WindowState>,
    input: ResMut<InputState>,
    input_config: Res<InputConfig>,
    modes: Res<ModeStack>,

    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,
    command_sender: ResMut<CommandSender>,
) {
    get!(window, mut input, input_config, modes);

    let mode = modes.get_mode();
    if mode == 'c' {
        return;
    }

    let mut consumed = false;
    if mode == 'i' {
        for event in window.events() {
            let Event::Key(KeyEvent {
                code: KeyCode::Char(chr),
                ..
            }) = event
            else {
                continue;
            };

            let registry = prefix_registry.get().await;

            let command = command_registry.get().await.parse_command(
                CommandRegistry::split_command(&format!("a \'{chr}\' false")),
                true,
                false,
                &registry,
                &modes,
            );
            if let Some(command) = command {
                command_sender.get().await.send(command).unwrap();
            }

            consumed = true;
        }
    }
    if consumed {
        return;
    }

    if window.events().is_empty() {
        return;
    }

    let mut found_num = false;

    for event in window.events() {
        let Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            ..
        }) = event
        else {
            continue;
        };

        if ch.is_numeric() {
            if *ch == '0' && input.repeat_count.is_empty() {
                continue;
            }
            input.repeat_count.push(*ch);
            found_num = true;
        }
    }

    if found_num {
        return;
    }

    let no_inputs = input.active_inputs.is_empty();
    let mut completed_input = false;

    let repeat_count = input.repeat_count.clone().parse().unwrap_or(1);
    get!(command_registry, prefix_registry, command_sender);
    input.active_inputs.retain_mut(|(idx, step)| {
        if completed_input {
            return false;
        }
        match input_config.inputs[*idx].step(&window, mode, *step) {
            InputResult::Failed => false,
            InputResult::Step => {
                *step += 1;
                true
            }
            InputResult::Complete => {
                completed_input = true;

                match &input_config.inputs[*idx].event {
                    InputEvent::Commands(c) => {
                        for _ in 0..repeat_count {
                            for command in c {
                                let command = command_registry.parse_command(
                                    CommandRegistry::split_command(command),
                                    true,
                                    false,
                                    &prefix_registry,
                                    &modes,
                                );
                                if let Some(command) = command {
                                    command_sender.send(command).unwrap();
                                } else {
                                    return false;
                                }
                            }
                        }
                    }
                }

                false
            }
        }
    });

    if completed_input {
        input.active_inputs.clear();
        return;
    }

    if !no_inputs {
        if input.active_inputs.is_empty() {
            input.repeat_count.clear();
        }

        return;
    }

    for idx in 0..input_config.inputs.len() {
        match input_config.inputs[idx].step(&window, mode, 0) {
            InputResult::Step => input.active_inputs.push((idx, 1)),
            InputResult::Complete => {
                match &input_config.inputs[idx].event {
                    InputEvent::Commands(c) => {
                        'repeat: for _ in 0..repeat_count {
                            for command in c {
                                let command = command_registry.parse_command(
                                    CommandRegistry::split_command(command),
                                    true,
                                    false,
                                    &prefix_registry,
                                    &modes,
                                );
                                if let Some(command) = command {
                                    command_sender.send(command).unwrap();
                                } else {
                                    break 'repeat;
                                }
                            }
                        }
                    }
                }

                break;
            }
            InputResult::Failed => {}
        }
    }

    if input.active_inputs.is_empty() {
        input.repeat_count.clear();
    }
}
