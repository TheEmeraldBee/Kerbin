use crate::*;
use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};
use kerbin_macros::State;

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
        .row(flexible(), vec![])
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
        render!(&mut desc_chunk, (0, desc_chunk.size().y - 1) => ["─".repeat(desc_chunk.size().x as usize - 1)]);

        render!(&mut desc_chunk, (0, 0) => [desc_buffer]);
    }
}
