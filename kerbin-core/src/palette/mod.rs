use std::sync::Arc;

use ascii_forge::prelude::*;

use crate::State;

pub mod ranking;
pub use ranking::*;

#[derive(Default)]
pub struct CommandPaletteState {
    pub old_input: String,
    pub input: String,
    pub completion: Option<String>,
    pub suggestions: Vec<Buffer>,

    pub desc: Option<Buffer>,

    pub input_valid: bool,
}

pub fn update_palette_suggestions(state: Arc<State>) {
    let mode = state.get_mode();
    if mode != 'c' {
        return;
    }

    let mut palette = state.palette.write().unwrap();
    if palette.old_input != palette.input {
        palette.old_input = palette.input.clone();
        (palette.suggestions, palette.completion, palette.desc) =
            state.get_command_suggestions(&palette.input);
    }

    palette.input_valid = state.validate_command(&palette.input);
}

pub fn handle_command_palette_input(state: Arc<State>) {
    let window = state.window.read().unwrap();

    let mut palette = state.palette.write().unwrap();

    let mode = state.get_mode();
    if mode != 'c' {
        return;
    }

    for event in window.events() {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char(c) => palette.input.push(c),
                KeyCode::Backspace => {
                    palette.input.pop();
                }
                KeyCode::Enter => {
                    state.pop_mode();

                    state.call_command(&palette.input);
                    palette.input.clear();
                }
                KeyCode::Esc => {
                    state.pop_mode();
                    palette.input.clear();
                }
                KeyCode::Tab => {
                    if let Some(completion) = palette.completion.take() {
                        palette.input = completion;
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn render_command_palette(state: Arc<State>) {
    let mut window = state.window.write().unwrap();
    let palette = state.palette.read().unwrap();
    let mode = state.get_mode();

    if mode != 'c' {
        return;
    }

    let size = window.size();
    let command_line_y = size.y.saturating_sub(1);

    let suggestion_count = palette.suggestions.len().min(5);
    let desc_height = palette.desc.as_ref().map_or(0, |b| b.size().y);

    let mut total_palette_height = 2;

    if suggestion_count > 0 {
        total_palette_height += suggestion_count as u16;
    }
    if desc_height > 0 {
        total_palette_height += 1;
        total_palette_height += desc_height;
    }

    render!(window, (0, command_line_y) => [" ".repeat(size.x as usize)]);

    for i in 0..=total_palette_height.saturating_sub(2) {
        render!(window, (0, size.y.saturating_sub(i)) => [" ".repeat(size.x as usize)]);
    }

    let theme = state.theme.read().unwrap();
    let style = if palette.input_valid {
        theme
            .get("ui.commandline.valid")
            .unwrap_or(ContentStyle::default().green())
    } else {
        theme
            .get("ui.commandline.invalid")
            .unwrap_or(ContentStyle::default().red())
    };
    render!(window, (0, command_line_y) => [":", StyledContent::new(style, palette.input.as_str())]);

    let mut current_y = command_line_y.saturating_sub(2);

    if suggestion_count > 0 {
        let suggestions_start_y =
            current_y.saturating_sub(suggestion_count.saturating_sub(1) as u16);
        for i in 0..suggestion_count {
            render!(window, (2, suggestions_start_y + i as u16) => [palette.suggestions[i]]);
        }
        current_y = suggestions_start_y.saturating_sub(1);
    }

    if let Some(desc_buffer) = &palette.desc {
        render!(window, (2, current_y) => ["─".repeat(size.x as usize - 2)]);
        current_y = current_y.saturating_sub(1);

        let desc_render_y = current_y.saturating_sub(desc_height.saturating_sub(1));
        render!(window, (2, desc_render_y) => [desc_buffer]);
    }
}
