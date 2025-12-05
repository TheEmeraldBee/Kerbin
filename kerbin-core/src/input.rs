use crate::ascii_forge::prelude::*;
use crate::*;
use ascii_forge::widgets::Border;
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
    pub desc: String,
}

#[derive(State, Default)]
pub struct InputState {
    pub repeat_count: String,

    pub tree: KeyTree<Vec<String>, Metadata>,
}

/// Registers the help menu chunk in the UI if there are active input sequences.
pub async fn register_help_menu_chunk(
    window: Res<WindowState>,
    chunks: ResMut<Chunks>,
    input: Res<InputState>,
) {
    get!(input);

    if input.tree.active_tree().is_none() {
        return;
    }

    get!(mut chunks, window);

    // Place a layout in the bottom right corner
    let rect = Layout::new()
        .row(flexible(), vec![flexible()])
        .row(
            // Ensure space for all active inputs (+2 for border)
            max(input.tree.collect_layer_metadata().unwrap().len() as u16 + 2),
            vec![flexible(), percent(50.0), fixed(1)],
        )
        .row(fixed(1), vec![flexible()])
        .calculate(window.size())
        .unwrap()[1][1];

    // This must render above the buffer, or the 0 z-index
    chunks.register_chunk::<HelpChunk>(1, rect);
}

/// Renders the help menu, displaying currently active input sequences.
pub async fn render_help_menu(chunk: Chunk<HelpChunk>, input: Res<InputState>) {
    get!(input);
    if input.tree.active_tree().is_none() {
        return;
    }

    let mut chunk = &mut chunk.get().await.unwrap();

    let border = Border::square(chunk.size().x, chunk.size().y);

    render!(&mut chunk, (0, 0) => [border]);

    let metadata = input.tree.collect_layer_metadata().unwrap();

    // Render up to the chunk's height (-2 on size for border)
    for (i, data) in metadata
        .iter()
        .enumerate()
        .take(metadata.len().min(chunk.size().y as usize - 2))
    {
        render!(&mut chunk, vec2(1, 1 + i as u16) => [ data.0.to_string(), "   ", data.1.as_ref().map(|x| x.desc.as_str()).unwrap_or_default() ]);
    }
}

/// Handles incoming key events, processes input sequences, and dispatches commands.
pub async fn handle_inputs(
    window: Res<WindowState>,
    input: ResMut<InputState>,
    modes: Res<ModeStack>,

    command_registry: Res<CommandRegistry>,
    prefix_registry: Res<CommandPrefixRegistry>,
    command_sender: ResMut<CommandSender>,

    log: Res<LogSender>,
) {
    get!(window, mut input, modes, log);

    if window.events().is_empty() {
        return;
    }

    for event in window.events() {
        if let Event::Paste(text) = event {
            let registry = prefix_registry.get().await;
            let command = command_registry.get().await.parse_command(
                vec!["a".to_string(), text.to_string(), "false".to_string()],
                true,
                false,
                None,
                false,
                &registry,
                &modes,
            );
            if let Some(command) = command {
                command_sender.get().await.send(command).unwrap();
            }
        }
    }

    let mode = modes.get_mode();
    if mode == 'c' {
        return;
    }

    let mut consumed = false;
    if mode == 'i' {
        for event in window.events() {
            if let Event::Key(KeyEvent {
                code: KeyCode::Char(chr),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            }) = event
            {
                consumed = true;

                // Existing character handling
                let registry = prefix_registry.get().await;
                let command = command_registry.get().await.parse_command(
                    vec!["a".to_string(), chr.to_string(), "false".to_string()],
                    true,
                    false,
                    None,
                    false,
                    &registry,
                    &modes,
                );
                if let Some(command) = command {
                    command_sender.get().await.send(command).unwrap();
                }
            }
        }
    }

    if consumed {
        return;
    }

    let mut found_num = false;

    for event in window.events() {
        let Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            ..
        }) = event
        else {
            continue;
        };

        if ch.is_numeric() {
            if *ch == '0' && input.repeat_count.is_empty() {
                continue;
            }
            input.repeat_count.push(*ch);
            found_num = true;
        }
    }

    if found_num {
        return;
    }

    let resolver_engine = resolver_engine().await;
    let resolver = resolver_engine.as_resolver();

    // Update the tree
    for event in window.events() {
        let Event::Key(event) = event else {
            continue;
        };
        match input
            .tree
            .step(&resolver, event.code, event.modifiers, |data| {
                let Some(data) = data else {
                    return true;
                };

                // Check if modes are satisfied
                let mode_ok =
                    data.modes.is_empty() || data.modes.iter().any(|x| modes.mode_on_stack(*x));

                // Check if invalid modes are present
                let invalid_mode_present =
                    data.invalid_modes.iter().any(|x| modes.mode_on_stack(*x));

                // Check if required templates are present
                let templates_ok = data.required_templates.is_empty()
                    || data
                        .required_templates
                        .iter()
                        .all(|x| resolver_engine.has_template(x));

                mode_ok && !invalid_mode_present && templates_ok
            }) {
            Ok(StepResult::Success(sequence, commands)) => {
                drop(resolver);
                drop(resolver_engine);

                let mut resolver = resolver_engine_mut().await;
                for (i, key) in sequence.iter().enumerate() {
                    resolver.set_template(i, [key.to_string()]);
                }

                let resolver = resolver.as_resolver();
                'outer: for _ in 0..input.repeat_count.parse::<i32>().unwrap_or(1) {
                    for command in &commands {
                        let registry = prefix_registry.get().await;
                        let command = command_registry.get().await.parse_command(
                            word_split(command),
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
                input.repeat_count.clear();

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
