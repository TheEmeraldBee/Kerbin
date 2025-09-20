use crate::*;
use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};
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
///
/// This system runs when the editor is in command palette mode ('c'). It compares
/// the current input to the old input to determine if suggestions need to be re-calculated
/// and if the input's validity needs to be re-evaluated.
///
/// # Arguments
///
/// * `modes`: `Res<ModeStack>` to check if the editor is in command palette mode.
/// * `palette`: `ResMut<CommandPaletteState>` for mutable access to the palette's state.
/// * `prefix_registry`: `Res<CommandPrefixRegistry>` for validating commands with prefixes.
/// * `commands`: `Res<CommandRegistry>` for generating command suggestions and validating commands.
/// * `theme`: `Res<Theme>` for styling command suggestions.
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

/// Handles keyboard input specific to the command palette ('c' mode).
///
/// This system processes key events (characters, backspace, enter, escape, tab)
/// when the command palette is active, updating the palette's input string
/// and dispatching commands when `Enter` is pressed.
///
/// # Arguments
///
/// * `window`: `Res<WindowState>` to read key events.
/// * `palette`: `ResMut<CommandPaletteState>` for mutable access to the palette's input.
/// * `modes`: `ResMut<ModeStack>` for popping the command palette mode when finished.
/// * `command_registry`: `Res<CommandRegistry>` for parsing the entered command.
/// * `prefix_registry`: `Res<CommandPrefixRegistry>` for applying command prefixes during parsing.
/// * `command_sender`: `ResMut<CommandSender>` for dispatching the parsed command.
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

/// Registers the command palette's required chunks for rendering.
///
/// This system dynamically creates or updates drawing chunks for the command line input,
/// suggestions, and command description, adjusting their sizes based on content.
/// It only runs when the editor is in command palette mode.
///
/// # Arguments
///
/// * `chunks`: `ResMut<Chunks>` for registering drawing chunks.
/// * `window`: `Res<WindowState>` to get the total window size for layout calculation.
/// * `modes`: `Res<ModeStack>` to check if command palette mode is active.
/// * `palette`: `Res<CommandPaletteState>` to get current palette state (e.g., number of suggestions, description height).
pub async fn register_command_palette_chunks(
    chunks: ResMut<Chunks>,
    window: Res<WindowState>,
    modes: Res<ModeStack>,

    palette: Res<CommandPaletteState>,
) {
    let modes = modes.get();

    if !modes.mode_on_stack('c') {
        return;
    }

    let palette = palette.get();

    let mut chunks = chunks.get();
    let window = window.get();

    let mut desc_height = 0;
    if let Some(h) = palette.desc.as_ref().map(|x| x.size().y) {
        desc_height += h + 1
    }

    let mut sug_height = 0;
    if !palette.suggestions.is_empty() {
        sug_height = palette.suggestions.len().min(5) as u16
    }

    let layout = Layout::new()
        // Take up the whole top
        .row(flexible(), vec![flexible()])
        .row(fixed(desc_height), vec![flexible()])
        .row(fixed(sug_height), vec![flexible()])
        // Take up a row for a gap
        .row(fixed(1), vec![flexible()])
        .row(fixed(1), vec![flexible()])
        .calculate(window.size())
        .unwrap();

    if desc_height != 0 {
        chunks.register_chunk::<CommandDescChunk>(2, layout[1][0]);
    }

    if sug_height != 0 {
        chunks.register_chunk::<CommandSuggestionsChunk>(2, layout[2][0]);
    }

    chunks.register_chunk::<CommandlineChunk>(2, layout[4][0]);
}

/// Renders the command palette to the window, including input, suggestions, and description.
///
/// This system draws the current command line input, applies styling based on its
/// validity, renders command suggestions, and displays the description of the top suggestion.
/// It also places the cursor within the command input line.
///
/// # Arguments
///
/// * `line_chunk`: `Chunk<CommandlineChunk>` for the command line input area.
/// * `suggestions_chunk`: `Chunk<CommandSuggestionsChunk>` for the suggestions area.
/// * `desc_chunk`: `Chunk<CommandDescChunk>` for the command description area.
/// * `palette`: `Res<CommandPaletteState>` for the current state of the palette.
/// * `modes`: `Res<ModeStack>` to check if the editor is in command palette mode.
/// * `theme`: `Res<Theme>` for retrieving styling information.
pub async fn render_command_palette(
    line_chunk: Chunk<CommandlineChunk>,

    suggestions_chunk: Chunk<CommandSuggestionsChunk>,
    desc_chunk: Chunk<CommandDescChunk>,

    palette: Res<CommandPaletteState>,
    modes: Res<ModeStack>,

    theme: Res<Theme>,
) {
    let palette = palette.get();
    let modes = modes.get();
    let theme = theme.get();

    if modes.get_mode() != 'c' {
        return;
    }

    let mut line_chunk = line_chunk.get().unwrap();

    line_chunk.set_cursor(
        1,
        vec2(palette.input.len() as u16 + 1, 0),
        SetCursorStyle::SteadyBar,
    );

    let style = if palette.input_valid {
        theme
            .get("ui.commandline.valid")
            .unwrap_or(ContentStyle::default().green())
    } else {
        theme
            .get("ui.commandline.invalid")
            .unwrap_or(ContentStyle::default().red())
    };
    render!(&mut line_chunk, (0, 0) => [":", StyledContent::new(style, palette.input.as_str())]);

    let suggestion_count = palette.suggestions.len();
    if let Some(mut suggestions_chunk) = suggestions_chunk.get()
        && suggestion_count > 0
    {
        for i in 0..suggestion_count.min(suggestions_chunk.size().y as usize + 1) {
            render!(&mut suggestions_chunk, (0, i as u16) => [palette.suggestions[i]]);
        }
    }

    if let Some(mut desc_chunk) = desc_chunk.get()
        && let Some(desc_buffer) = &palette.desc
    {
        render!(&mut desc_chunk, (0, desc_chunk.size().y - 1) => ["─".repeat(desc_chunk.size().x as usize)]);

        render!(&mut desc_chunk, (0, 0) => [desc_buffer]);
    }
}
