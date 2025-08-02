use std::{rc::Rc, str::FromStr};

use ascii_forge::{prelude::*, widgets::border::Border};
use crokey::{Combiner, KeyCombination};
use rune::{Value, runtime::Function};
use stategine::prelude::*;

use crate::{SpecialCommand, commands::EditorCommand, key_check::KeyCheckExt, mode::Mode};

pub enum InputResult {
    Failed,
    Step,
    Complete,
}

#[derive(Debug)]
pub struct Input {
    pub valid_modes: Vec<char>,
    pub key_sequence: Vec<KeyCombination>,
    pub func: Rc<Function>,
    pub description: String,
}

impl Input {
    pub fn step(
        &self,
        window: &Window,
        combiner: &mut Combiner,
        mode: char,
        step: usize,
    ) -> InputResult {
        if !self.valid_modes.contains(&mode) && !self.valid_modes.is_empty() {
            return InputResult::Failed;
        }

        if window.combination(combiner, self.key_sequence[step]) {
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
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join("")
    }
}
#[derive(Default, rune::Any)]
pub struct InputConfig {
    pub inputs: Vec<Input>,
}

const MAX_DESC_LEN: usize = 30;

impl InputConfig {
    #[rune::function(keep)]
    pub fn register_input(
        &mut self,
        modes: Vec<char>,
        sequence: Vec<String>,
        func: Function,
        desc: String,
    ) {
        let desc = desc.to_string();

        let mut key_sequence = vec![];
        for key in sequence {
            match KeyCombination::from_str(&key) {
                Ok(t) => key_sequence.push(t),
                Err(e) => {
                    tracing::error!("Failed to add key: `{e}`");
                    return;
                }
            }
        }

        if desc.len() > MAX_DESC_LEN {
            panic!("Description `{desc}` is too long");
        }

        self.inputs.push(Input {
            valid_modes: modes,
            key_sequence,
            func: Rc::new(func),
            description: desc,
        });
    }
}

#[derive(Default)]
pub struct InputState {
    pub repeat_count: String,
    pub active_inputs: Vec<(usize, usize)>,
}

pub fn render_help_menu(
    mut window: ResMut<Window>,
    input: Res<InputState>,
    input_config: Res<InputConfig>,
) {
    if input.active_inputs.is_empty() {
        return;
    }

    let border = Border::square(40, 12);

    for i in 0..input.active_inputs.len().min(10) {
        let active = input.active_inputs[i];
        let binding = &input_config.inputs[active.0];
        render!(window, window.size() - vec2(39, 14 - i as u16) => [ binding.sequence_str(active.1), " - ", binding.description ]);
    }

    render!(window, window.size() - vec2(40, 15) => [border]);
}

pub fn handle_inputs(
    mut commands: ResMut<Commands>,
    window: Res<Window>,
    mut combiner: ResMut<Combiner>,
    mut input: ResMut<InputState>,
    input_config: Res<InputConfig>,
    mode: Res<Mode>,
) {
    if mode.0 == 'c' {
        return;
    }

    let mut consumed = false;
    if mode.0 == 'i' {
        for event in window.events() {
            let Event::Key(KeyEvent {
                code: KeyCode::Char(chr),
                ..
            }) = event
            else {
                continue;
            };

            commands.add(EditorCommand::InsertChar(*chr));
            commands.add(EditorCommand::MoveCursor(1, 0));
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
        match input_config.inputs[*idx].step(&window, &mut combiner, mode.0, *step) {
            InputResult::Failed => false,
            InputResult::Step => {
                *step += 1;
                true
            }
            InputResult::Complete => {
                completed_input = true;

                commands.add(SpecialCommand::RunFunction(
                    input_config.inputs[*idx].func.clone(),
                    Value::from(repeat_count as u64),
                ));

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
        match check.step(&window, &mut combiner, mode.0, 0) {
            InputResult::Step => input.active_inputs.push((i, 1)),
            InputResult::Complete => {
                commands.add(SpecialCommand::RunFunction(
                    check.func.clone(),
                    Value::from(repeat_count as u64),
                ));

                break;
            }
            InputResult::Failed => {}
        }
    }

    if input.active_inputs.is_empty() {
        input.repeat_count = "".to_string();
    }
}
