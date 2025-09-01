use std::collections::VecDeque;

use ascii_forge::{prelude::*, widgets::border::Border};
use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};

use crate::{CommandPrefixRegistry, CommandRegistry, CommandSender, ModeStack};

pub enum InputResult {
    Failed,
    Step,
    Complete,
}

pub enum InputEvent {
    Commands(Vec<String>),
    //Func(Box<dyn Fn(Arc<State>, usize) -> bool + Send + Sync>),
}

pub struct Input {
    pub valid_modes: Vec<char>,
    pub invalid_modes: Vec<char>,
    pub key_sequence: Vec<(KeyModifiers, KeyCode)>,
    pub event: InputEvent,

    pub desc: String,
}

/// Wierd ass function that is required thanks to crossterm being kinda wierd
pub fn check_key_code(a: KeyCode, b: KeyCode) -> bool {
    match (a, b) {
        (KeyCode::Char(aa), KeyCode::Char(bb)) => aa.eq_ignore_ascii_case(&bb),
        _ => a == b,
    }
}

impl Input {
    //pub fn new(
    //    modes: impl IntoIterator<Item = char>,
    //invalid_modes: impl IntoIterator<Item = char>,
    //sequence: impl IntoIterator<Item = (KeyModifiers, KeyCode)>,
    //event: Box<dyn Fn(Arc<State>, usize) -> bool + Send + Sync>,
    //
    //desc: String,
    //) -> Self {
    //    Self {
    //        valid_modes: modes.into_iter().collect(),
    //        invalid_modes: invalid_modes.into_iter().collect(),
    //        key_sequence: sequence.into_iter().collect(),
    //        event: InputEvent::Func(event),
    //
    //        desc,
    //    }
    //}
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

#[derive(Default)]
pub struct InputConfig {
    inputs: VecDeque<Input>,
}

impl InputConfig {
    pub fn register_input(&mut self, input: Input) {
        self.inputs.push_front(input)
    }
}

#[derive(Default)]
pub struct InputState {
    pub(crate) repeat_count: String,
    pub(crate) active_inputs: Vec<(usize, usize)>,
}

pub async fn render_help_menu(
    window: ResMut<Window>,
    input: Res<InputState>,
    input_config: Res<InputConfig>,
) {
    let mut window = window.get();
    let input = input.get();
    let input_config = input_config.get();

    if input.active_inputs.is_empty() {
        return;
    }

    let border = Border::square(40, 12);

    for i in 0..input.active_inputs.len().min(10) {
        let active = input.active_inputs[i];
        let binding = &input_config.inputs[active.0];
        render!(window, window.size() - vec2(39, 14 - i as u16) => [ binding.sequence_str(active.1), " - ", binding.desc ]);
    }

    render!(window, window.size() - vec2(40, 15) => [border]);
}

pub async fn handle_inputs(
    window: Res<Window>,
    input: ResMut<InputState>,
    input_config: Res<InputConfig>,
    modes: Res<ModeStack>,

    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,
    command_sender: ResMut<CommandSender>,
) {
    let window = window.get();
    let mut input = input.get();
    let input_config = input_config.get();
    let modes = modes.get();

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

            let command = command_registry.get().parse_command(
                CommandRegistry::split_command(&format!("a \'{chr}\' false")),
                true,
                true,
                &prefix_registry.get(),
                &modes,
            );
            if let Some(command) = command {
                command_sender.get().send(command).unwrap();
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
            // We don't care about 0 if you press it when string is empty
            // You can't trigger an event 0 times. Might as well allow that as a keybind!
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
                    //InputEvent::Func(f) => {
                    //f(state.clone(), repeat_count);
                    //}
                    InputEvent::Commands(c) => {
                        for _ in 0..repeat_count {
                            for command in c {
                                let command = command_registry.get().parse_command(
                                    CommandRegistry::split_command(command),
                                    true,
                                    true,
                                    &prefix_registry.get(),
                                    &modes,
                                );
                                if let Some(command) = command {
                                    command_sender.get().send(command).unwrap();
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
                    //InputEvent::Func(f) => {
                    //f(state.clone(), repeat_count);
                    //}
                    InputEvent::Commands(c) => {
                        'repeat: for _ in 0..repeat_count {
                            for command in c {
                                let command = command_registry.get().parse_command(
                                    CommandRegistry::split_command(command),
                                    true,
                                    true,
                                    &prefix_registry.get(),
                                    &modes,
                                );
                                if let Some(command) = command {
                                    command_sender.get().send(command).unwrap();
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
