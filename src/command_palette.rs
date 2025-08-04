use crate::{AppState, buffer_extensions::BufferExtension, commands::EditorCommand};
use ascii_forge::prelude::*;
use std::{
    str::SplitWhitespace,
    sync::{Arc, atomic::Ordering},
};

/// A descriptor for a command that can be executed from the palette.
struct CommandInfo {
    /// Valid names for the command (w/write, o/open)
    valid_names: Vec<String>,
    /// A function that parses arguments and returns a set of EditorCommands
    parser: fn(SplitWhitespace) -> Result<Vec<EditorCommand>, String>,
}

impl CommandInfo {
    pub fn new(
        valid_names: impl IntoIterator<Item = impl ToString>,
        parser: fn(SplitWhitespace) -> Result<Vec<EditorCommand>, String>,
    ) -> Self {
        Self {
            valid_names: valid_names
                .into_iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
            parser,
        }
    }

    pub fn suggestion_string(&self) -> String {
        if self.valid_names.len() > 1 {
            format!(
                "{} ({})",
                self.valid_names[0],
                self.valid_names
                    .iter()
                    .skip(1)
                    .map(|x| x.to_string())
                    .reduce(|acc, x| format!("{acc}, {x}"))
                    .unwrap_or_default()
            )
        } else {
            self.valid_names[0].to_string()
        }
    }
}

pub struct CommandPaletteState {
    /// The current text entered by the user.
    pub input: String,
    /// A list of command names that match the current input.
    pub suggestions: Vec<String>,
    /// The master list of all available commands.
    commands: Vec<CommandInfo>,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPaletteState {
    pub fn new() -> Self {
        let commands = vec![
            CommandInfo::new(["w", "write"], |mut args| {
                Ok(vec![EditorCommand::WriteFile(
                    args.next().map(|s| s.to_string()),
                )])
            }),
            CommandInfo::new(["wq", "write-quit"], |mut args| {
                Ok(vec![
                    EditorCommand::WriteFile(args.next().map(|s| s.to_string())),
                    EditorCommand::Quit,
                ])
            }),
            CommandInfo::new(["q", "quit"], |mut _args| Ok(vec![EditorCommand::Quit])),
            CommandInfo::new(["o", "open"], |mut args| {
                if let Some(path) = args.next() {
                    Ok(vec![EditorCommand::OpenFile(path.to_string())])
                } else {
                    Err("open command requires a path".to_string())
                }
            }),
            CommandInfo::new(["lo", "log-open"], |mut _args| {
                Ok(vec![EditorCommand::OpenFile("zellix.log".to_string())])
            }),
            CommandInfo::new(["c", "close"], |mut _args| {
                Ok(vec![EditorCommand::CloseCurrentBuffer])
            }),
            CommandInfo::new(["r", "reload"], |mut _args| {
                Ok(vec![EditorCommand::RefreshHighlights])
            }),
        ];
        Self {
            input: String::new(),
            suggestions: Vec::new(),
            commands,
        }
    }

    /// Filters the command list based on the current input.
    fn update_suggestions(&mut self) {
        self.suggestions.clear();
        if self.input.is_empty() {
            return;
        }
        let input_lower = self.input.to_lowercase();
        for cmd in &self.commands {
            for name in &cmd.valid_names {
                if name.starts_with(&input_lower) {
                    self.suggestions.push(cmd.suggestion_string());
                    break;
                }
            }
        }
    }

    /// Parses and executes the current input string.
    fn execute(&self, state: Arc<AppState>) {
        let mut parts = self.input.split_whitespace();
        if let Some(cmd_name) = parts.next() {
            if let Some(command_info) = self
                .commands
                .iter()
                .find(|c| c.valid_names.contains(&cmd_name.to_string()))
            {
                match (command_info.parser)(parts) {
                    Ok(cmds) => {
                        for cmd in cmds {
                            state.commands.send(cmd).unwrap();
                        }
                    }
                    Err(_e) => {
                        // Optionally, display the error to the user
                    }
                }
            }
        }
    }
}

/// Handles user input when the command palette is active.
pub fn handle_command_palette_input(state: Arc<AppState>) {
    let window = state.window.read().unwrap();

    let mut palette = state.palette.write().unwrap();

    let mode = state.get_mode();
    if mode != 'c' {
        return;
    }

    let mut executed = false;
    for event in window.events() {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char(c) => palette.input.push(c),
                KeyCode::Backspace => {
                    palette.input.pop();
                }
                KeyCode::Enter => {
                    palette.execute(state.clone());
                    executed = true;
                }
                KeyCode::Esc => executed = true,
                _ => {}
            }
        }
    }

    if executed {
        palette.input.clear();
        state.mode.store(u32::from('n'), Ordering::Relaxed); // Return to normal mode
    }

    palette.update_suggestions();
}

/// Renders the command palette UI at the bottom of the screen.
pub fn render_command_palette(state: Arc<AppState>) {
    let mut window = state.window.write().unwrap();

    let palette = state.palette.read().unwrap();

    let mode = char::from_u32(state.mode.load(Ordering::Relaxed)).unwrap();

    if mode != 'c' {
        return;
    }

    let size = window.size();
    let bottom_y = size.y.saturating_sub(2);

    // Render suggestions above the input line
    let suggestion_count = palette.suggestions.len().min(5);
    for i in 0..suggestion_count {
        let y = bottom_y.saturating_sub(suggestion_count as u16) + i as u16;
        // Clear the line's text
        render!(window, (2, y) => [" ".repeat(window.size().x as usize / 2)]);
        render!(window, (2, y) => [palette.suggestions[i]]);

        window.buffer_mut().style_line(y, |s| {
            s.on(Color::Rgb {
                r: 40,
                g: 40,
                b: 56,
            })
        })
    }

    // Render the input line
    render!(window, (0, window.size().y - 1) => [":", palette.input.as_str()]);
}
