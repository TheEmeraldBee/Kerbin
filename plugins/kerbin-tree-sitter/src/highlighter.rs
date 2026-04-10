use crate::{
    grammar_manager::GrammarManager,
    query_walker::{QueryWalker, QueryWalkerBuilder},
    state::{TreeSitterState, emit_spans},
};
use std::{cmp::Ordering, collections::BinaryHeap, ops::Range};

use kerbin_core::*;
use ratatui::style::Style;
use tree_sitter::QueryProperty;

pub fn get_capture_priority(query: &tree_sitter::Query, pattern_index: usize) -> i64 {
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

pub fn is_conceal_pattern(query: &tree_sitter::Query, pattern_index: usize) -> bool {
    query
        .general_predicates(pattern_index)
        .iter()
        .any(|p| p.operator.as_ref() == "conceal!")
}

pub fn conceal_scope_from_query(query: &tree_sitter::Query, pattern_index: usize) -> ConcealScope {
    for pred in query.general_predicates(pattern_index) {
        if pred.operator.as_ref() == "conceal!" {
            for arg in &pred.args {
                if let tree_sitter::QueryPredicateArg::String(s) = arg
                    && s.as_ref() == "line"
                {
                    return ConcealScope::Line;
                }
            }
        }
    }
    ConcealScope::Byte
}

/// Returns `(trim_before, trim_after)` for a conceal pattern.
///
/// Supported `conceal!` arguments:
/// - `"trim"` — trim whitespace on both sides
/// - `"trim-before"` — trim whitespace before the concealed range
/// - `"trim-after"` — trim whitespace after the concealed range
pub fn conceal_trim_from_query(query: &tree_sitter::Query, pattern_index: usize) -> (bool, bool) {
    for pred in query.general_predicates(pattern_index) {
        if pred.operator.as_ref() == "conceal!" {
            let mut trim_before = false;
            let mut trim_after = false;
            for arg in &pred.args {
                if let tree_sitter::QueryPredicateArg::String(s) = arg {
                    match s.as_ref() {
                        "trim" => return (true, true),
                        "trim-before" => trim_before = true,
                        "trim-after" => trim_after = true,
                        _ => {}
                    }
                }
            }
            return (trim_before, trim_after);
        }
    }
    (false, false)
}

fn capture_specificity(name: &str) -> usize {
    name.matches('.').count()
}

pub fn translate_name_to_style(theme: &Theme, mut name: &str) -> Style {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub byte_range: Range<usize>,
    pub capture_name: String,
    pub priority: i64,
    pub capture_index: u32,
    pub is_conceal: bool,
    pub conceal_scope: ConcealScope,
    pub trim_before: bool,
    pub trim_after: bool,
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
            let is_conceal = is_conceal_pattern(query, entry.query_match.pattern_index);
            let conceal_scope = conceal_scope_from_query(query, entry.query_match.pattern_index);
            let (trim_before, trim_after) =
                conceal_trim_from_query(query, entry.query_match.pattern_index);
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
                    is_conceal,
                    conceal_scope,
                    trim_before,
                    trim_after,
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
        if let Some(start) = prev_pos
            && pos > start
            && let Some(top) = active.peek()
        {
            result.push(HighlightSpan {
                byte_range: start..pos,
                capture_name: top.span.capture_name.clone(),
                priority: top.span.priority,
                capture_index: top.span.capture_index,
                is_conceal: top.span.is_conceal,
                conceal_scope: top.span.conceal_scope,
                trim_before: top.span.trim_before,
                trim_after: top.span.trim_after,
            });
        }

        if is_start {
            active.push(Active { span });
        } else {
            // Remove finished span (lazy removal)
            active.retain(|a| !std::ptr::eq(a.span, span));
        }

        prev_pos = Some(pos);
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
    let Some(mut buf) = buffers.cur_text_buffer_mut().await else {
        return;
    };

    if buf.byte_changes.is_empty() {
        return;
    }

    let Some(state) = buf.get_state_mut::<TreeSitterState>().await else {
        return;
    };

    let namespace = "tree-sitter::highlights";
    buf.renderer.clear_extmark_ns(namespace);

    let Some(highlighter) =
        Highlighter::new(&config_path.0, &mut grammars, &state, buf.get_rope())
    else {
        return;
    };

    emit_spans(highlighter.collect_spans(), namespace, &mut buf, &theme);
}

