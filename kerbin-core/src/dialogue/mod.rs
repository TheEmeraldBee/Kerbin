use crate::*;
use kerbin_macros::State;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

#[derive(Default, Clone, Debug, PartialEq)]
pub enum InputKind {
    #[default]
    Str,
    Cmd,
    Commands,
}

/// Core state for handling the dialogue input overlay
#[derive(Default, State)]
pub struct DialogueState {
    pub active: bool,
    pub title: String,
    pub desc: String,
    pub input: String,
    pub input_kind: InputKind,
    /// Tokens to execute on submit; each element should be a Token::List representing one command
    pub commands: Vec<Token>,
    /// Tokens to execute on each input change; each element should be a Token::List
    pub on_change: Vec<Token>,
    /// Name of the resolver template variable set to the user's input on submit/change
    pub var_name: String,
    pub input_valid: bool,
}

/// Executes the on-change commands with `%var_name` set to the current input.
/// Called after each `DialoguePush` / `DialoguePop`.
pub async fn run_dialogue_on_change(
    state: &mut State,
    on_change: &[Token],
    var_name: &str,
    input: &str,
) {
    resolver_engine_mut().await.set_template(var_name, input);

    if on_change.is_empty() {
        return;
    }

    for token in on_change {
        if let Token::List(cmd_tokens) = token {
            let command = state.lock_state::<CommandRegistry>().await.parse_command(
                cmd_tokens.clone(),
                true,
                false,
                Some(&resolver_engine().await.as_resolver()),
                true,
                &*state.lock_state::<CommandPrefixRegistry>().await,
                &*state.lock_state::<ModeStack>().await,
            );
            if let Some(cmd) = command
                && let Err(e) = state.lock_state::<CommandSender>().await.send(cmd) {
                    tracing::error!("dialogue on_change: failed to send command: {e}");
                }
        }
    }
}

pub async fn update_dialogue_validation(
    modes: Res<ModeStack>,
    dialogue: ResMut<DialogueState>,
    prefix_registry: Res<CommandPrefixRegistry>,
    commands: Res<CommandRegistry>,
) {
    get!(modes, mut dialogue, prefix_registry, commands);

    if !dialogue.active {
        return;
    }

    if dialogue.input_kind == InputKind::Str {
        dialogue.input_valid = true;
        return;
    }

    dialogue.input_valid = commands.validate_command(
        &dialogue.input,
        Some(&resolver_engine().await.as_resolver()),
        &prefix_registry,
        &modes,
    );
}

pub async fn register_dialogue_chunk(
    chunks: ResMut<Chunks>,
    window: Res<WindowState>,
    dialogue: Res<DialogueState>,
) {
    get!(dialogue);
    if !dialogue.active {
        return;
    }
    get!(mut chunks, window);

    let size = window.size();
    let has_desc = !dialogue.desc.is_empty();
    let height = if has_desc { 4u16 } else { 3u16 };

    let [_, center_row, _] = Layout::vertical([
        Constraint::Fill(2),
        Constraint::Length(height),
        Constraint::Fill(3),
    ])
    .areas(size);

    let [_, area, _] = Layout::horizontal([
        Constraint::Percentage(20),
        Constraint::Percentage(60),
        Constraint::Percentage(20),
    ])
    .areas(center_row);

    chunks.register_chunk::<DialogueChunk>(3, area);
}

pub async fn render_dialogue(
    chunk: Chunk<DialogueChunk>,
    dialogue: Res<DialogueState>,
    theme: Res<Theme>,
) {
    get!(dialogue, theme);

    if !dialogue.active {
        return;
    }

    let Some(mut chunk) = chunk.get().await else {
        return;
    };

    let border_style = theme.get_fallback_default(["ui.commandline.border", "ui.text"]);
    let title_style = theme.get_fallback_default(["ui.commandline.title", "ui.text"]);
    let icon_style = theme.get_fallback_default(["ui.commandline.icon", "ui.text"]);
    let desc_style = theme.get_fallback_default(["ui.commandline.desc", "ui.text"]);

    let area = chunk.area();

    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" {} ", dialogue.title), title_style))
        .border_style(border_style)
        .render(area, &mut chunk);

    let mut row = area.y + 1;

    if !dialogue.desc.is_empty() {
        let desc_rect = Rect::new(area.x + 2, row, area.width.saturating_sub(4), 1);
        Paragraph::new(Span::styled(dialogue.desc.clone(), desc_style))
            .render(desc_rect, &mut chunk);
        row += 1;
    }

    let (icon, style) = if dialogue.input.is_empty() {
        (
            "●",
            theme.get_fallback_default(["ui.commandline.prompt", "ui.text"]),
        )
    } else if dialogue.input_valid {
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
    chunk.set_string(inner_x + 1, row, icon, icon_style);
    chunk.set_string(inner_x + 2, row, " : ", Style::default());
    chunk.set_string(inner_x + 5, row, &dialogue.input, style);

    let cursor_x = area.x + dialogue.input.len() as u16 + 6;
    chunk.set_cursor(1, cursor_x, row, CursorShape::BlinkingBar);
}
