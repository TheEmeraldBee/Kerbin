use crate::{
    grammar_manager::GrammarManager,
    query_walker::{QueryWalker, QueryWalkerBuilder},
    state::TreeSitterState,
};
use std::{cmp::Ordering, collections::BinaryHeap, ops::Range};

use kerbin_core::{ascii_forge::window::ContentStyle, *};
use tree_sitter::QueryProperty;

pub fn get_capture_priority(query: &tree_sitter::Query, pattern_index: usize) -> i64 {
    // Default base priority: pattern order
    let mut priority = pattern_index as i64;

    for QueryProperty { key, value, .. } in query.property_settings(pattern_index) {
        if key.as_ref() == "priority"
            && let Some(value) = value
            && let Ok(num) = value.parse::<i64>()
        {
            priority = num;
        }
    }

    priority
}

fn capture_specificity(name: &str) -> usize {
    name.matches('.').count()
}

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
    pub priority: i64,
    pub capture_index: u32,
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
        let walker = QueryWalkerBuilder::new(state, rope, query)
            .with_injected_queries(injected)
            .build();
        Some(Self { walker })
    }

    pub fn collect_spans(mut self) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();

        self.walker.walk(|entry| {
            let query = &entry.query;
            for capture in entry.query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let range = capture.node.byte_range().start + entry.byte_offset
                    ..capture.node.byte_range().end + entry.byte_offset;

                let base_priority = get_capture_priority(query, entry.query_match.pattern_index);
                let specificity = capture_specificity(capture_name);
                let priority = base_priority * 10
                    + specificity as i64
                    + match entry.is_injected {
                        true => 500,
                        false => 0,
                    };

                spans.push(HighlightSpan {
                    byte_range: range,
                    capture_name: capture_name.to_string(),
                    priority,
                    capture_index: capture.index,
                });
            }
            true
        });

        spans
    }
}

#[derive(Debug)]
struct Active<'a> {
    span: &'a HighlightSpan,
}

impl<'a> PartialEq for Active<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.span.priority == other.span.priority
            && self.span.capture_index == other.span.capture_index
    }
}
impl<'a> Eq for Active<'a> {}

impl<'a> PartialOrd for Active<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<'a> Ord for Active<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse so BinaryHeap gives us the *highest* priority first
        self.span
            .priority
            .cmp(&other.span.priority)
            .then(self.span.capture_index.cmp(&other.span.capture_index))
    }
}

pub fn merge_overlapping_spans(mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Sort spans by start position first (so we can sweep left-to-right)
    spans.sort_by_key(|s| s.byte_range.start);

    // Collect all unique split points (span starts + ends)
    let mut points = Vec::with_capacity(spans.len() * 2);
    for s in &spans {
        points.push((s.byte_range.start, true, s)); // true = start
        points.push((s.byte_range.end, false, s)); // false = end
    }
    points.sort_by_key(|(pos, _, _)| *pos);

    let mut active: BinaryHeap<Active> = BinaryHeap::new();
    let mut result = Vec::new();
    let mut prev_pos: Option<usize> = None;

    for (pos, is_start, span) in points {
        if let Some(start) = prev_pos {
            if pos > start {
                if let Some(top) = active.peek() {
                    // Emit a segment with the currently active top span
                    result.push(HighlightSpan {
                        byte_range: start..pos,
                        capture_name: top.span.capture_name.clone(),
                        priority: top.span.priority,
                        capture_index: top.span.capture_index,
                    });
                }
            }
        }

        // Update active spans
        if is_start {
            active.push(Active { span });
        } else {
            // Remove finished span (lazy removal)
            active = active
                .into_iter()
                .filter(|a| a.span as *const _ != span as *const _)
                .collect();
        }

        prev_pos = Some(pos);
    }

    result
}

/// Calculates the affected range that needs re-highlighting based on byte changes
/// Returns a byte range, or None if the entire file should be re-highlighted
fn calculate_affected_range(
    byte_changes: &[[((usize, usize), usize); 3]],
    rope: &ropey::Rope,
) -> Option<Range<usize>> {
    if byte_changes.is_empty() {
        return None;
    }

    // Find the earliest start and latest end of all changes
    let mut min_start = usize::MAX;
    let mut max_end = 0;

    for change in byte_changes {
        let start = change[0].1;
        let new_end = change[2].1;

        min_start = min_start.min(start);
        max_end = max_end.max(new_end);
    }

    // Expand the range to include complete lines for better context
    // This helps catch cases where syntax depends on line boundaries
    let start_line = rope.byte_to_line_idx(min_start, ropey::LineType::LF_CR);
    let end_line = rope.byte_to_line_idx(max_end, ropey::LineType::LF_CR);

    // Add some padding lines for context (e.g., 5 lines before and after)
    let padding_lines = 5;
    let start_line_with_padding = start_line.saturating_sub(padding_lines);
    let end_line_with_padding =
        (end_line + padding_lines).min(rope.len_lines(ropey::LineType::LF_CR).saturating_sub(1));

    let range_start = rope.line_to_byte_idx(start_line_with_padding, ropey::LineType::LF_CR);
    let range_end = if end_line_with_padding + 1 < rope.len_lines(ropey::LineType::LF_CR) {
        rope.line_to_byte_idx(end_line_with_padding + 1, ropey::LineType::LF_CR)
    } else {
        rope.len()
    };

    Some(range_start..range_end)
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

    // Calculate the affected range
    let affected_range = calculate_affected_range(&buf.byte_changes, &buf.rope);

    let namespace = "tree-sitter::highlights";

    // If we have a specific range to update, only re-highlight that portion
    if let Some(range) = affected_range {
        // Remove extmarks in the affected range
        buf.renderer.remove_extmarks_in_range(namespace, &range);

        // Create a highlighter WITHOUT byte range constraint
        // We need to query the whole file because tree-sitter nodes from outside
        // the affected range might extend into it
        let Some((query, injected)) = grammars.get_query_set(&config_path.0, "highlights", &state)
        else {
            return;
        };

        let mut walker = QueryWalkerBuilder::new(&state, &buf.rope, query)
            .with_injected_queries(injected)
            .build();

        let mut spans = Vec::new();
        walker.walk(|entry| {
            let query = &entry.query;
            for capture in entry.query_match.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let node_range = capture.node.byte_range().start + entry.byte_offset
                    ..capture.node.byte_range().end + entry.byte_offset;

                // Only collect spans that intersect with our affected range
                if node_range.start < range.end && node_range.end > range.start {
                    let base_priority =
                        get_capture_priority(query, entry.query_match.pattern_index);
                    let specificity = capture_specificity(capture_name);
                    let priority = base_priority * 10 + specificity as i64;

                    spans.push(HighlightSpan {
                        byte_range: node_range,
                        capture_name: capture_name.to_string(),
                        priority,
                        capture_index: capture.index,
                    });
                }
            }
            true
        });

        // Add the new extmarks for the affected range
        for span in merge_overlapping_spans(spans) {
            let hl_style = translate_name_to_style(&theme, &span.capture_name);

            buf.add_extmark(
                ExtmarkBuilder::new_range(namespace, span.byte_range.clone())
                    .with_priority(span.priority as i32)
                    .with_decoration(ExtmarkDecoration::Highlight { hl: hl_style }),
            );
        }
    } else {
        // Full re-highlight (fallback for complex changes or first highlight)
        buf.renderer.clear_extmark_ns(namespace);

        let Some(highlighter) = Highlighter::new(&config_path.0, &mut grammars, &state, &buf.rope)
        else {
            return;
        };

        let spans = highlighter.collect_spans();

        for span in merge_overlapping_spans(spans) {
            let hl_style = translate_name_to_style(&theme, &span.capture_name);

            buf.add_extmark(
                ExtmarkBuilder::new_range(namespace, span.byte_range.clone())
                    .with_priority(span.priority as i32)
                    .with_decoration(ExtmarkDecoration::Highlight { hl: hl_style }),
            );
        }
    }
}
