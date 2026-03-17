use crate::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};
use kerbin_macros::State;

pub mod ranking;
pub use ranking::*;

/// Core state for handling command palette
#[derive(Default, State)]
pub struct CommandPaletteState {
    /// Input string from previous frame
    pub old_input: String,
    /// Current user input string
    pub input: String,
    /// Optional auto-completion for current input
    pub completion: Option<String>,
    /// List of command suggestions as Lines
    pub suggestions: Vec<Line<'static>>,

    /// Detailed description of top suggestion as Lines
    pub desc: Option<Vec<Line<'static>>>,

    /// Whether current input is valid
    pub input_valid: bool,
}

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
        (palette.suggestions, palette.completion, palette.desc) = commands
            .get_command_suggestions(&palette.input, &theme)
            .await;
    }

    palette.input_valid = commands.validate_command(
        &palette.input,
        Some(&resolver_engine().await.as_resolver()),
        &prefix_registry,
        &modes,
    );
}

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
        .map(|lines| lines.len() as u16 + 2)
        .unwrap_or(0);

    let sug_height = if !palette.suggestions.is_empty() {
        (palette.suggestions.len().min(5) as u16) + 2
    } else {
        0
    };

    let input_height = 3u16;

    // Vertical layout: top pad, desc, suggestions, input, bottom pad
    let [_top, desc_row, sug_row, input_row, _bottom] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(desc_height),
        Constraint::Length(sug_height),
        Constraint::Length(input_height),
        Constraint::Percentage(40),
    ])
    .areas(window_size);

    // Horizontal centering for each row
    let center_constraints = [
        Constraint::Percentage(20),
        Constraint::Percentage(60),
        Constraint::Percentage(20),
    ];

    if desc_height != 0 {
        let [_, desc_area, _] = Layout::horizontal(center_constraints).areas(desc_row);
        chunks.register_chunk::<CommandDescChunk>(2, desc_area);
    }
    if sug_height != 0 {
        let [_, sug_area, _] = Layout::horizontal(center_constraints).areas(sug_row);
        chunks.register_chunk::<CommandSuggestionsChunk>(2, sug_area);
    }
    let [_, input_area, _] = Layout::horizontal(center_constraints).areas(input_row);
    chunks.register_chunk::<CommandlineChunk>(2, input_area);
}

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

    let area = line_chunk.area();

    // Render input border
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(" Command ", title_style))
        .border_style(border_style)
        .render(area, &mut line_chunk);

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
                .unwrap_or(Style::default().green()),
        )
    } else {
        (
            "✗",
            theme
                .get("ui.commandline.invalid")
                .unwrap_or(Style::default().red()),
        )
    };

    let inner_x = area.x + 1;
    let inner_y = area.y + 1;

    line_chunk.set_string(inner_x + 1, inner_y, icon, icon_style);
    line_chunk.set_string(inner_x + 2, inner_y, " : ", Style::default());
    line_chunk.set_string(inner_x + 5, inner_y, &palette.input, style);

    // Set cursor position inside input
    let cursor_x = area.x + palette.input.len() as u16 + 6;
    let cursor_y = area.y + 1;
    line_chunk.set_cursor(1, cursor_x, cursor_y, CursorShape::BlinkingBar);

    // Render suggestions
    let suggestion_count = palette.suggestions.len();
    if let Some(mut suggestions_chunk) = suggestions_chunk.get().await
        && suggestion_count > 0
    {
        let sug_area = suggestions_chunk.area();

        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .render(sug_area, &mut suggestions_chunk);

        let max_display =
            suggestion_count.min((sug_area.height.saturating_sub(2)) as usize);
        let inner_width = sug_area.width.saturating_sub(6);

        for i in 0..max_display {
            let row = i as u16 + 1;
            let sug_x = sug_area.x + 4;
            let sug_y = sug_area.y + row;

            if i == 0 {
                suggestions_chunk.set_string(sug_area.x + 1, sug_y, "▶", icon_style);
            }

            let sug_rect = Rect::new(sug_x, sug_y, inner_width, 1);
            Paragraph::new(palette.suggestions[i].clone())
                .render(sug_rect, &mut suggestions_chunk);
        }
    }

    // Render description
    if let Some(mut desc_chunk) = desc_chunk.get().await
        && let Some(desc_lines) = &palette.desc
    {
        let desc_area = desc_chunk.area();

        Block::bordered()
            .border_type(BorderType::Rounded)
            .title(Span::styled(" Description ", title_style))
            .border_style(border_style)
            .render(desc_area, &mut desc_chunk);

        let inner = Rect::new(
            desc_area.x + 2,
            desc_area.y + 1,
            desc_area.width.saturating_sub(4),
            desc_area.height.saturating_sub(2),
        );

        Paragraph::new(desc_lines.clone()).render(inner, &mut desc_chunk);
    }
}
