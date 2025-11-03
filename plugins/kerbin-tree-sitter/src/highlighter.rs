use crate::{grammar_manager::GrammarManager, query_walker::QueryWalker, state::TreeSitterState};
use std::ops::Range;

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
    pub depth: u32,
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
        let walker = QueryWalker::new_with_injected_queries(state, rope, query, injected);
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
                let priority = base_priority * 10 + specificity as i64;

                // Calculate node depth efficiently by walking up the tree
                let mut depth = 0u32;
                let mut node = capture.node;
                while let Some(parent) = node.parent() {
                    depth += 1;
                    node = parent;
                }

                spans.push(HighlightSpan {
                    byte_range: range,
                    capture_name: capture_name.to_string(),
                    priority,
                    depth,
                    capture_index: capture.index,
                });
            }
            true
        });

        spans
    }
}

pub fn merge_overlapping_spans(spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Collect all split points (start and end positions)
    let mut split_points = std::collections::BTreeSet::new();
    for span in &spans {
        split_points.insert(span.byte_range.start);
        split_points.insert(span.byte_range.end);
    }

    let split_points: Vec<usize> = split_points.into_iter().collect();

    // For each segment between split points, find the highest priority span
    let mut segments: Vec<Option<(String, i64, u32, u32)>> = Vec::new();

    for i in 0..split_points.len().saturating_sub(1) {
        let seg_start = split_points[i];
        let seg_end = split_points[i + 1];

        // Find all spans that cover this segment
        let mut best_span: Option<(String, i64, u32, u32)> = None;

        for span in &spans {
            // Check if this span covers the segment
            if span.byte_range.start <= seg_start && span.byte_range.end >= seg_end {
                let is_better = if let Some((_, best_priority, best_depth, best_index)) = &best_span
                {
                    // Compare: priority first, then depth, then capture_index
                    span.priority > *best_priority
                        || (span.priority == *best_priority && span.depth > *best_depth)
                        || (span.priority == *best_priority
                            && span.depth == *best_depth
                            && span.capture_index > *best_index)
                } else {
                    true
                };

                if is_better {
                    best_span = Some((
                        span.capture_name.clone(),
                        span.priority,
                        span.depth,
                        span.capture_index,
                    ));
                }
            }
        }

        segments.push(best_span);
    }

    // Merge consecutive segments with the same style into spans
    let mut result = Vec::new();
    let mut current_start = None;
    let mut current_style: Option<(String, i64, u32, u32)> = None;

    for (i, segment_style) in segments.iter().enumerate() {
        let seg_start = split_points[i];

        match (&current_style, segment_style) {
            (
                Some((cur_name, cur_pri, cur_depth, cur_index)),
                Some((seg_name, seg_pri, seg_depth, seg_index)),
            ) if cur_name == seg_name
                && cur_pri == seg_pri
                && cur_depth == seg_depth
                && cur_index == seg_index =>
            {
                // Continue current span
            }
            (Some((name, priority, depth, index)), _) => {
                // End current span and start new one
                if let Some(start) = current_start {
                    result.push(HighlightSpan {
                        byte_range: start..seg_start,
                        capture_name: name.clone(),
                        priority: *priority,
                        depth: *depth,
                        capture_index: *index,
                    });
                }
                current_start = segment_style.as_ref().map(|_| seg_start);
                current_style = segment_style.clone();
            }
            (None, Some(_)) => {
                // Start new span
                current_start = Some(seg_start);
                current_style = segment_style.clone();
            }
            (None, None) => {
                // No span active
            }
        }
    }

    // Don't forget the last span
    if let (Some(start), Some((name, priority, depth, index))) = (current_start, current_style) {
        result.push(HighlightSpan {
            byte_range: start..*split_points.last().unwrap(),
            capture_name: name,
            priority,
            depth,
            capture_index: index,
        });
    }

    result
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

    let spans = highlighter.collect_spans();

    let renderer = &mut buf.renderer;
    renderer.clear_extmark_ns("tree-sitter::highlights");

    for span in merge_overlapping_spans(spans) {
        let hl_style = translate_name_to_style(&theme, &span.capture_name);

        renderer.add_extmark_range(
            "tree-sitter::highlights",
            span.byte_range.clone(),
            span.priority as i32,
            vec![ExtmarkDecoration::Highlight { hl: hl_style }],
        );
    }
}
