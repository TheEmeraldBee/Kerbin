use ascii_forge::prelude::*;
use kerbin_macros::State;
use kerbin_state_machine::storage::*;
use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};

use crate::{CommandPrefixRegistry, CommandRegistry, CommandSender, ModeStack, Theme, WindowState};

pub mod ranking;
pub use ranking::*;

#[derive(Default, State)]
pub struct CommandPaletteState {
    pub old_input: String,
    pub input: String,
    pub completion: Option<String>,
    pub suggestions: Vec<Buffer>,

    pub desc: Option<Buffer>,

    pub input_valid: bool,
}

pub async fn update_palette_suggestions(
    modes: Res<ModeStack>,
    palette: ResMut<CommandPaletteState>,
    prefix_registry: Res<CommandPrefixRegistry>,
    commands: Res<CommandRegistry>,

    theme: Res<Theme>,
) {
    let modes = modes.get();
    let mut palette = palette.get();
    let commands = commands.get();

    let prefix_registry = prefix_registry.get();

    let theme = theme.get();

    if modes.get_mode() != 'c' {
        return;
    }

    if palette.old_input != palette.input {
        palette.old_input = palette.input.clone();
        (palette.suggestions, palette.completion, palette.desc) =
            commands.get_command_suggestions(&palette.input, &theme);
    }

    palette.input_valid = commands.validate_command(&palette.input, &prefix_registry, &modes);
}

pub async fn handle_command_palette_input(
    window: Res<WindowState>,
    palette: ResMut<CommandPaletteState>,
    modes: ResMut<ModeStack>,

    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,

    command_sender: ResMut<CommandSender>,
) {
    let window = window.get();
    let mut palette = palette.get();
    let mut modes = modes.get();

    let mode = modes.get_mode();
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
                    modes.pop_mode();

                    let command = command_registry.get().parse_command(
                        CommandRegistry::split_command(&palette.input),
                        true,
                        true,
                        &prefix_registry.get(),
                        &modes,
                    );
                    if let Some(command) = command {
                        command_sender.get().send(command).unwrap();
                    }

                    palette.input.clear();
                }
                KeyCode::Esc => {
                    modes.pop_mode();
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

pub async fn render_command_palette(
    window: ResMut<WindowState>,
    palette: Res<CommandPaletteState>,
    modes: Res<ModeStack>,

    theme: Res<Theme>,
) {
    let mut window = window.get();
    let palette = palette.get();
    let modes = modes.get();
    let theme = theme.get();

    if modes.get_mode() != 'c' {
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
