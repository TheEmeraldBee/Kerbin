use crate::*;
use ascii_forge::{prelude::*, widgets::Border, window::crossterm::cursor::SetCursorStyle};
use kerbin_macros::State;

pub mod ranking;
pub use ranking::*;

/// The internal state of the command palette.
///
/// This struct holds all information related to the command palette's current
/// display and input processing, including user input, command suggestions,
/// and validation status.
#[derive(Default, State)]
pub struct CommandPaletteState {
    /// The input string from the previous frame, used to detect changes and
    /// optimize suggestion updates.
    pub old_input: String,
    /// The current input string typed by the user in the command palette.
    pub input: String,
    /// An optional string representing a potential auto-completion for the current input.
    pub completion: Option<String>,
    /// A vector of `Buffer`s, each representing a command suggestion to display.
    pub suggestions: Vec<Buffer>,

    /// An optional `Buffer` containing the detailed description of the top command suggestion.
    pub desc: Option<Buffer>,

    /// A boolean indicating whether the current input string forms a valid command.
    pub input_valid: bool,
}

/// System used to update the command palette's suggestions and input validation status.
pub async fn update_palette_suggestions(
    modes: Res<ModeStack>,
    palette: ResMut<CommandPaletteState>,
    prefix_registry: Res<CommandPrefixRegistry>,
    commands: Res<CommandRegistry>,
    theme: Res<Theme>,
) {
    get!(modes, mut palette, prefix_registry, commands, theme);

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

/// Handles keyboard input specific to the command palette ('c' mode).
pub async fn handle_command_palette_input(
    window: Res<WindowState>,
    palette: ResMut<CommandPaletteState>,
    modes: ResMut<ModeStack>,
    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,
    command_sender: ResMut<CommandSender>,
) {
    get!(window, mut palette, mut modes);

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

                    let registry = prefix_registry.get().await;
                    let command = command_registry.get().await.parse_command(
                        CommandRegistry::split_command(&palette.input),
                        true,
                        true,
                        &registry,
                        &modes,
                    );
                    if let Some(command) = command {
                        command_sender.get().await.send(command).unwrap();
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

/// Registers the command palette's required chunks for rendering.
/// Creates a centered floating layout similar to noice.nvim
pub async fn register_command_palette_chunks(
    chunks: ResMut<Chunks>,
    window: Res<WindowState>,
    modes: Res<ModeStack>,
    palette: Res<CommandPaletteState>,
) {
    get!(modes);
    if !modes.mode_on_stack('c') {
        return;
    }

    get!(palette, mut chunks, window);

    let window_size = window.size();

    // Calculate content heights
    let desc_height = palette
        .desc
        .as_ref()
        .map(|x| x.size().y + 3) // +3 for borders and padding
        .unwrap_or(0);

    let sug_height = if !palette.suggestions.is_empty() {
        (palette.suggestions.len().min(5) as u16) + 2 // +2 for borders
    } else {
        0
    };

    let input_height = 3; // Input with borders

    // Create centered layout using flexible for padding and percent for width
    let layout = Layout::new()
        // Top padding (flexible to center vertically)
        .row(flexible(), vec![flexible()])
        // Content area with horizontal centering
        .row(
            max(desc_height),
            vec![percent(20.0), percent(60.0), percent(20.0)],
        )
        .row(
            max(sug_height),
            vec![percent(20.0), percent(60.0), percent(20.0)],
        )
        .row(
            max(input_height),
            vec![percent(20.0), percent(60.0), percent(20.0)],
        )
        // Bottom padding (flexible to center vertically)
        .row(percent(40.0), vec![flexible()])
        .calculate(window_size)
        .unwrap();

    // Register chunks in the centered column
    if desc_height != 0 {
        chunks.register_chunk::<CommandDescChunk>(2, layout[1][1]);
    }
    if sug_height != 0 {
        chunks.register_chunk::<CommandSuggestionsChunk>(2, layout[2][1]);
    }
    chunks.register_chunk::<CommandlineChunk>(2, layout[3][1]);
}

/// Renders the command palette with noice.nvim-inspired styling.
pub async fn render_command_palette(
    line_chunk: Chunk<CommandlineChunk>,
    suggestions_chunk: Chunk<CommandSuggestionsChunk>,
    desc_chunk: Chunk<CommandDescChunk>,
    palette: Res<CommandPaletteState>,
    modes: Res<ModeStack>,
    theme: Res<Theme>,
) {
    get!(palette, modes, theme);

    if modes.get_mode() != 'c' {
        return;
    }

    let mut line_chunk = line_chunk.get().await.unwrap();

    // Theme styles
    let border_style = theme.get_fallback_default(["ui.commandline.border", "ui.text"]);
    let title_style = theme.get_fallback_default(["ui.commandline.title", "ui.text"]);
    let icon_style = theme.get_fallback_default(["ui.commandline.icon", "ui.text"]);

    let width = line_chunk.size().x as usize;
    let height = line_chunk.size().y;

    // Fill interior with spaces
    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            line_chunk.set(vec2(x as u16, y), " ");
        }
    }

    let mut border =
        Border::rounded(width as u16, height).with_title(title_style.apply(" Command "));
    border.style = border_style;

    render!(line_chunk, (0, 0) => [ border ]);

    // Determine icon and style based on validity
    let (icon, style) = if palette.input.is_empty() {
        (
            "●",
            theme.get_fallback_default(["ui.commandline.prompt", "ui.text"]),
        )
    } else if palette.input_valid {
        (
            "✓",
            theme
                .get("ui.commandline.valid")
                .unwrap_or(ContentStyle::default().green()),
        )
    } else {
        (
            "✗",
            theme
                .get("ui.commandline.invalid")
                .unwrap_or(ContentStyle::default().red()),
        )
    };

    render!(&mut line_chunk, (1, 1) => [
        " ",
        StyledContent::new(icon_style, icon),
        " : ",
        StyledContent::new(style, &palette.input),
    ]);

    line_chunk.set_cursor(
        1,
        vec2(palette.input.len() as u16 + 6, 1),
        SetCursorStyle::SteadyBar,
    );

    // Render suggestions with enhanced styling
    let suggestion_count = palette.suggestions.len();
    if let Some(mut suggestions_chunk) = suggestions_chunk.get().await
        && suggestion_count > 0
    {
        let width = suggestions_chunk.size().x;
        let height = suggestions_chunk.size().y;

        // Fill interior with spaces
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                suggestions_chunk.set(vec2(x as u16, y), " ");
            }
        }

        let mut border = Border::rounded(width, height);
        border.style = border_style;

        // Top border for suggestions
        render!(&mut suggestions_chunk, (0, 0) => [
            border
        ]);

        let max_display =
            suggestion_count.min((suggestions_chunk.size().y.saturating_sub(2)) as usize);

        for i in 0..max_display {
            let row = i as u16 + 1;

            // Highlight first suggestion (selected)
            if i == 0 {
                render!(&mut suggestions_chunk, (1, row) => [
                    StyledContent::new(icon_style, "▶"),
                ]);

                let sug_x = 4;
                render!(&mut suggestions_chunk, (sug_x, row) => [palette.suggestions[i]]);
            } else {
                let sug_x = 4;
                render!(&mut suggestions_chunk, (sug_x, row) => [palette.suggestions[i]]);
            }
        }
    }

    // Render description with border and title
    if let Some(mut desc_chunk) = desc_chunk.get().await
        && let Some(desc_buffer) = &palette.desc
    {
        let width = desc_chunk.size().x;
        let height = desc_chunk.size().y;

        // Fill interior with spaces
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                desc_chunk.set(vec2(x as u16, y), " ");
            }
        }

        let mut border =
            Border::rounded(width, height).with_title(title_style.apply(" Description "));
        border.style = border_style;

        render!(&mut desc_chunk, (0, 0) => [
            border
        ]);

        render!(&mut desc_chunk, (2, 1) => [desc_buffer]);
    }
}
