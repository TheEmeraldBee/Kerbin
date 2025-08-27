use std::sync::Arc;

use ascii_forge::prelude::*;

use crate::State;

#[derive(Default)]
pub struct CommandPaletteState {
    pub old_input: String,
    pub input: String,
    pub suggestions: Vec<Buffer>,

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
        palette.suggestions = state.get_command_suggestions(&palette.input);
    }

    palette.input_valid = state.validate_command(&palette.input);
}

/// Handles user input when the command palette is active.
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
    let bottom_y = size.y.saturating_sub(2);

    // Render suggestions above the input line
    let suggestion_count = palette.suggestions.len().min(5);
    for i in 0..suggestion_count {
        let y = bottom_y.saturating_sub(suggestion_count as u16) + i as u16;

        render!(window, (2, y) => [" ".repeat(size.x as usize / 2)]);
        render!(window, (2, y) => [palette.suggestions[i]]);
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

    render!(window, (0, size.y - 1) => [":", StyledContent::new(style, palette.input.as_str())]);
}
