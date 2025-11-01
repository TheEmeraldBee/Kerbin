use crate::{grammar_manager::GrammarManager, query_walker::QueryWalker, state::TreeSitterState};
use std::ops::Range;

use kerbin_core::{ascii_forge::window::ContentStyle, *};

/// Translates a capture name into a style
pub fn translate_name_to_style(theme: &Theme, mut name: &str) -> ContentStyle {
    loop {
        if let Some(value) = theme.get(&format!("ts.{name}")) {
            return value;
        }

        if let Some(last_dot_index) = name.rfind('.') {
            name = &name[..last_dot_index];
        } else {
            break;
        }
    }

    theme.get("ui.text").unwrap_or_default()
}

/// Represents a highlighted span
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub byte_range: Range<usize>,
    pub capture_name: String,
}

/// A highlight event - either pushing or popping a highlight from the stack
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HighlightEvent {
    /// Push a new highlight onto the stack at this byte position
    Push {
        byte_pos: usize,
        capture_name: String,
    },
    /// Pop a highlight from the stack at this byte position
    Pop { byte_pos: usize },
}

pub struct Highlighter<'tree, 'rope> {
    walker: QueryWalker<'tree, 'rope>,
}

impl<'tree, 'rope> Highlighter<'tree, 'rope> {
    pub fn new(
        config_path: &str,
        grammar_manager: &mut GrammarManager,
        state: &'tree TreeSitterState,
        rope: &'rope ropey::Rope,
    ) -> Option<Self> {
        let (query, injected) = grammar_manager.get_query_set(config_path, "highlights", state)?;
        let walker = QueryWalker::new_with_injected_queries(state, rope, query, injected);
        Some(Self { walker })
    }

    /// Generate highlight events in byte position order
    /// These events can be processed with a stack to determine the active highlight at any position
    pub fn events(mut self) -> Vec<HighlightEvent> {
        let mut events = Vec::new();

        // Collect all captures
        self.walker.walk(|entry| {
            for capture in entry.query_match.captures {
                let capture_name = entry.query.capture_names()[capture.index as usize];

                events.push(HighlightEvent::Push {
                    byte_pos: capture.node.byte_range().start,
                    capture_name: capture_name.to_string(),
                });

                events.push(HighlightEvent::Pop {
                    byte_pos: capture.node.byte_range().end,
                });
            }

            true
        });

        // Sort events by position, with Pop events before Push events at the same position
        events.sort_by(|a, b| {
            let pos_a = match a {
                HighlightEvent::Push { byte_pos, .. } => *byte_pos,
                HighlightEvent::Pop { byte_pos } => *byte_pos,
            };
            let pos_b = match b {
                HighlightEvent::Push { byte_pos, .. } => *byte_pos,
                HighlightEvent::Pop { byte_pos } => *byte_pos,
            };

            pos_a.cmp(&pos_b).then_with(|| {
                // Pop before Push at same position
                match (a, b) {
                    (HighlightEvent::Pop { .. }, HighlightEvent::Push { .. }) => {
                        std::cmp::Ordering::Less
                    }
                    (HighlightEvent::Push { .. }, HighlightEvent::Pop { .. }) => {
                        std::cmp::Ordering::Greater
                    }
                    _ => std::cmp::Ordering::Equal,
                }
            })
        });

        events
    }
}

pub async fn highlight_file(
    buffers: ResMut<Buffers>,
    grammars: ResMut<GrammarManager>,
    config_path: Res<ConfigFolder>,

    theme: Res<Theme>,
) {
    get!(mut buffers, mut grammars, config_path, theme);
    let mut buf = buffers.cur_buffer_mut().await;
    if buf.byte_changes.is_empty() {
        return;
    }

    let Some(state) = buf.get_state::<TreeSitterState>().await else {
        return;
    };

    let Some(highlighter) = Highlighter::new(&config_path.0, &mut grammars, &state, &buf.rope)
    else {
        return;
    };

    let events = highlighter.events();

    let renderer = &mut buf.renderer;
    renderer.clear_extmark_ns("tree-sitter::highlights");

    // Stack tracks (capture_name, start_byte_pos)
    let mut style_stack: Vec<(String, usize)> = Vec::new();

    for event in events {
        match event {
            HighlightEvent::Push {
                byte_pos,
                capture_name,
            } => {
                // Push the new highlight onto the stack with its start position
                style_stack.push((capture_name, byte_pos));
            }
            HighlightEvent::Pop { byte_pos } => {
                // Pop and create the extmark with the complete range
                if let Some((capture_name, start_pos)) = style_stack.pop() {
                    // Get the style for this capture name
                    let hl_style = translate_name_to_style(&theme, &capture_name);

                    renderer.add_extmark_range(
                        "tree-sitter::highlights",
                        start_pos..byte_pos,
                        1,
                        vec![ExtmarkDecoration::Highlight { hl: hl_style }],
                    );
                }
            }
        }
    }
}
