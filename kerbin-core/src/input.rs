use std::sync::Arc;

use ascii_forge::{prelude::*, widgets::border::Border};

use crate::{Insert, State};

pub enum InputResult {
    Failed,
    Step,
    Complete,
}

pub struct Input {
    valid_modes: Vec<char>,
    key_sequence: Vec<(KeyModifiers, KeyCode)>,
    event: Box<dyn Fn(Arc<State>, usize) -> bool + Send + Sync>,

    desc: String,
}

impl Input {
    pub fn new(
        modes: impl IntoIterator<Item = char>,
        sequence: impl IntoIterator<Item = (KeyModifiers, KeyCode)>,
        event: Box<dyn Fn(Arc<State>, usize) -> bool + Send + Sync>,

        desc: String,
    ) -> Self {
        Self {
            valid_modes: modes.into_iter().collect(),
            key_sequence: sequence.into_iter().collect(),
            event,

            desc,
        }
    }
    pub fn step(&self, window: &Window, mode: char, step: usize) -> InputResult {
        if !self.valid_modes.contains(&mode) && !self.valid_modes.is_empty() {
            return InputResult::Failed;
        }

        let seq = &self.key_sequence[step];
        if event!(window, Event::Key(k) => k.modifiers == seq.0 && k.code == seq.1) {
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
            .map(|x| format!("{}{}", x.0, x.1))
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Default)]
pub struct InputConfig {
    inputs: Vec<Input>,
}

impl InputConfig {
    pub fn register_input(&mut self, input: Input) {
        self.inputs.push(input)
    }
}

#[derive(Default)]
pub struct InputState {
    pub(crate) repeat_count: String,
    pub(crate) active_inputs: Vec<(usize, usize)>,
}

pub fn render_help_menu(state: Arc<State>) {
    let input = state.input_state.read().unwrap();
    let input_config = state.input_config.read().unwrap();
    let mut window = state.window.write().unwrap();
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

pub fn handle_inputs(state: Arc<State>) {
    let window = state.window.read().unwrap();

    let mut input = state.input_state.write().unwrap();

    let input_config = state.input_config.read().unwrap();

    let mode = state.get_mode();
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

            let buffer = state.buffers.read().unwrap().cur_buffer();
            let mut cur_buffer = buffer.write().unwrap();

            let row = cur_buffer.row;
            let col = cur_buffer.col;

            cur_buffer.action(Insert {
                row,
                col,
                content: chr.to_string(),
            });

            cur_buffer.move_cursor(0, 1);

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

                (*input_config.inputs[*idx].event)(state.clone(), repeat_count);

                false
            }
        }
    });

    if completed_input {
        input.active_inputs.clear();
    }

    if !no_inputs {
        return;
    }

    for (i, check) in input_config.inputs.iter().enumerate() {
        match check.step(&window, mode, 0) {
            InputResult::Step => input.active_inputs.push((i, 1)),
            InputResult::Complete => {
                (*check.event)(state.clone(), repeat_count);

                break;
            }
            InputResult::Failed => {}
        }
    }

    if input.active_inputs.is_empty() {
        input.repeat_count = "".to_string();
    }
}
