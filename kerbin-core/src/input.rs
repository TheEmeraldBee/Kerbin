use crate::*;
use crossterm::event::{Event, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Keybinding {
    pub keys: Vec<UnresolvedKeyBind>,
    pub commands: Vec<String>,

    #[serde(flatten)]
    pub metadata: Metadata,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Metadata {
    #[serde(default)]
    pub modes: Vec<char>,

    #[serde(default)]
    pub invalid_modes: Vec<char>,

    #[serde(default)]
    pub required_templates: Vec<String>,

    #[serde(default)]
    pub deny_repeat: bool,

    #[serde(default)]
    pub desc: String,
}

#[derive(State, Default)]
pub struct InputState {
    pub repeat_count: String,

    pub tree: KeyTree<Vec<String>, Metadata>,
}

pub async fn register_help_menu_chunk(
    window: Res<WindowState>,
    chunks: ResMut<Chunks>,
    input: Res<InputState>,
) {
    get!(input);

    if input.tree.active_tree().is_none() {
        return;
    }

    let metadata = input.tree.collect_layer_metadata().unwrap();
    let menu_height = metadata.len() as u16 + 2;

    get!(mut chunks, window);

    let size = window.size();

    // Place help menu in bottom-right corner
    let [_top, middle, _bottom] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(menu_height),
        Constraint::Length(1),
    ])
    .areas(size);

    let [_left, help_area, _right] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Percentage(50),
        Constraint::Length(1),
    ])
    .areas(middle);

    chunks.register_chunk::<HelpChunk>(1, help_area);
}

pub async fn render_help_menu(chunk: Chunk<HelpChunk>, input: Res<InputState>) {
    get!(input);
    if input.tree.active_tree().is_none() {
        return;
    }

    let mut chunk = chunk.get().await.unwrap();
    let area = chunk.area();

    Block::bordered()
        .border_type(BorderType::Plain)
        .render(area, &mut chunk);

    let metadata = input.tree.collect_layer_metadata().unwrap();

    for (i, data) in metadata
        .iter()
        .enumerate()
        .take(metadata.len().min(area.height as usize - 2))
    {
        let key_str = data.0.to_string();
        let desc_str = data.1.as_ref().map(|x| x.desc.as_str()).unwrap_or_default();
        let line_text = format!("{}   {}", key_str, desc_str);
        chunk.set_string(
            area.x + 1,
            area.y + 1 + i as u16,
            &line_text,
            Style::default(),
        );
    }
}

pub async fn handle_inputs(
    events: Res<CrosstermEvents>,
    input: ResMut<InputState>,
    modes: Res<ModeStack>,

    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,
    command_sender: ResMut<CommandSender>,

    log: Res<LogSender>,
) {
    get!(events, mut input, modes, log);

    if events.0.is_empty() {
        return;
    }

    for event in &events.0 {
        if let Event::Paste(text) = event {
            command_sender
                .get()
                .await
                .send(Box::new(BufferCommand::Append {
                    text: text.clone(),
                    extend: false,
                }))
                .unwrap();
        }
    }

    let resolver_engine = resolver_engine().await;
    let resolver = resolver_engine.as_resolver();

    // Update the tree
    for event in &events.0 {
        let Event::Key(event) = event else {
            continue;
        };
        let event: &KeyEvent = event;
        match input
            .tree
            .step(&resolver, event.code, event.modifiers, |data| {
                let Some(data) = data else {
                    return Some(u32::MAX);
                };

                let mode_ok =
                    data.modes.is_empty() || data.modes.iter().any(|x| modes.mode_on_stack(*x));

                let invalid_mode_present =
                    data.invalid_modes.iter().any(|x| modes.mode_on_stack(*x));

                let templates_ok = data.required_templates.is_empty()
                    || data
                        .required_templates
                        .iter()
                        .all(|x| resolver_engine.has_template(x));

                if mode_ok && !invalid_mode_present && templates_ok {
                    Some(
                        data.modes
                            .iter()
                            .filter_map(|x| modes.where_on_stack(*x))
                            .max()
                            .map(|x| x as u32)
                            .unwrap_or(u32::MAX),
                    )
                } else {
                    None
                }
            }) {
            Ok(StepResult::Success(sequence, commands, meta)) => {
                drop(resolver);
                drop(resolver_engine);

                let mut resolver = resolver_engine_mut().await;
                for (i, key) in sequence.iter().enumerate() {
                    resolver.set_template(i, key.to_string());
                }

                let mut repeat = 1;
                if !input.repeat_count.is_empty() && !meta.map(|x| x.deny_repeat).unwrap_or(false) {
                    repeat = input.repeat_count.parse().unwrap_or(1).max(1);
                    input.repeat_count.clear();
                }

                let resolver = resolver.as_resolver();
                'outer: for _ in 0..repeat {
                    for command in &commands {
                        let registry = prefix_registry.get().await;
                        let command = command_registry.get().await.parse_command(
                            tokenize(command).unwrap_or_default(),
                            true,
                            false,
                            Some(&resolver),
                            true,
                            &registry,
                            &modes,
                        );
                        if let Some(command) = command {
                            command_sender.get().await.send(command).unwrap();
                        } else {
                            break 'outer;
                        }
                    }
                }

                input.tree.reset();

                break;
            }
            Ok(StepResult::Step) => {}
            Ok(StepResult::Reset) => {}
            Err(e) => {
                log.critical(
                    "input::step",
                    format!("Failed to step tree due to error: {e:?}"),
                );
            }
        }
    }
}
