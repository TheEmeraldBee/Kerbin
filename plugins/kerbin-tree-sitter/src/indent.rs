use kerbin_core::{buffer::SafeRopeAccess, *};
use std::{collections::HashMap, sync::Arc};
use tree_sitter::{Query, QueryPredicate, QueryPredicateArg};

use crate::{
    grammar_manager::GrammarManager,
    query_walker::{QueryMatchEntry, QueryWalkerBuilder},
    state::TreeSitterState,
};

pub async fn newline_intercept(cmd: &BufferCommand, state: &mut State) -> InterceptorResult {
    match cmd {
        BufferCommand::Append { text, .. } if text == "\n" => {}
        _ => return InterceptorResult::Allow,
    }

    newline_and_indent(state).await;
    InterceptorResult::Cancel
}

async fn newline_and_indent(state: &mut State) {
    let mut buffers = state.lock_state::<Buffers>().await;
    let mut grammars = state.lock_state::<GrammarManager>().await;
    let config_path = state.lock_state::<ConfigFolder>().await.0.clone();
    let auto_pairs = state.lock_state::<AutoPairs>().await;

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else {
        return;
    };

    let cursor_byte = buf.primary_cursor().get_cursor_byte();
    let current_line_idx = buf.byte_to_line_clamped(cursor_byte);

    let current_line_indent = get_line_indent(&buf, current_line_idx);

    let char_at = buf.byte_to_char(cursor_byte).and_then(|ci| buf.char(ci));
    let char_before_idx = buf
        .byte_to_char(cursor_byte)
        .and_then(|ci| ci.checked_sub(1));
    let char_before = char_before_idx.and_then(|ci| buf.char(ci));

    let query_byte = match (char_before, char_at) {
        (Some(open), Some(close))
            if open != close
                && auto_pairs
                    .find_by_open(open)
                    .map(|p| p.close == close)
                    .unwrap_or(false) =>
        {
            char_before_idx
                .and_then(|ci| buf.char_to_byte(ci))
                .unwrap_or(cursor_byte)
        }
        _ => cursor_byte,
    };
    drop(auto_pairs);

    buf.action(Insert {
        byte: cursor_byte,
        content: "\n".to_string(),
    });
    buf.move_bytes(1, false);

    let Some(ts_state) = buf.get_state_mut::<TreeSitterState>().await else {
        if !current_line_indent.is_empty() {
            buf.action(Insert {
                byte: cursor_byte + 1,
                content: current_line_indent.clone(),
            });
            buf.move_bytes(current_line_indent.len() as isize, false);
        }
        return;
    };

    let Some((query, injected)) = grammars.get_query_set(&config_path, "indents", &ts_state) else {
        if !current_line_indent.is_empty() {
            buf.action(Insert {
                byte: cursor_byte + 1,
                content: current_line_indent.clone(),
            });
            buf.move_bytes(current_line_indent.len() as isize, false);
        }
        return;
    };

    let indent_str = calculate_indent(
        &ts_state,
        &buf,
        query,
        injected,
        cursor_byte,
        query_byte,
        &current_line_indent,
    );

    if !indent_str.is_empty() {
        buf.action(Insert {
            byte: cursor_byte + 1,
            content: indent_str.clone(),
        });
        buf.move_bytes(indent_str.len() as isize, false);
    }
}

fn get_line_indent(buf: &TextBuffer, line_idx: usize) -> String {
    let line = buf.line_clamped(line_idx);
    let mut indent = String::new();
    for char in line.chars() {
        if char == ' ' || char == '\t' {
            indent.push(char);
        } else {
            break;
        }
    }
    indent
}

#[derive(Clone, Debug)]
enum CaptureKind {
    Indent,
    IndentAlways,
    Outdent,
    OutdentAlways,
    Extend,
    ExtendPreventOnce,

    Align { anchor_col: usize },
}

#[derive(Clone, Debug, PartialEq)]
enum Scope {
    /// Applies to lines **inside** the node (default for @indent).
    Tail,
    /// Applies to lines **starting** with the node (default for @outdent / @align).
    All,
}

#[derive(Clone, Debug)]
struct IndentCapture {
    kind: CaptureKind,
    scope: Scope,
}

fn calculate_indent(
    state: &TreeSitterState,
    buf: &TextBuffer,
    query: Arc<Query>,
    injected: HashMap<String, Arc<Query>>,
    cursor_byte: usize,
    query_byte: usize,
    fallback_indent: &str,
) -> String {
    let query_end_byte = query_byte
        .saturating_add(1)
        .min(buf.len().saturating_add(1));

    let captures = collect_indent_captures(state, buf.get_rope(), query, &injected, query_end_byte);

    let tree = match state.tree.as_ref() {
        Some(t) => t,
        None => return fallback_indent.to_string(),
    };
    let root = tree.root_node();

    let deepest = root
        .descendant_for_byte_range(cursor_byte, cursor_byte)
        .unwrap_or(root);

    let mut prevent_once_consumed = false;
    let mut extended_nodes: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (node_id, caps) in &captures {
        for cap in caps {
            match &cap.kind {
                CaptureKind::ExtendPreventOnce => {
                    prevent_once_consumed = true;
                }
                CaptureKind::Extend => {
                    if !prevent_once_consumed {
                        extended_nodes.insert(*node_id);
                    }
                    prevent_once_consumed = false;
                }
                _ => {}
            }
        }
    }

    let mut indent_delta: i32 = 0;
    let mut any_captures = false;
    let mut align_col: Option<usize> = None;

    let mut node = deepest;
    loop {
        let node_id = node.id();
        let start = node.start_byte();
        let end = node.end_byte();

        let cursor_inside =
            (extended_nodes.contains(&node_id) || cursor_byte < end) && cursor_byte >= start;

        if cursor_inside && let Some(caps) = captures.get(&node_id) {
            for cap in caps {
                let applies = match cap.scope {
                    Scope::All => is_first_in_line(node, buf),
                    Scope::Tail => true,
                };

                if applies {
                    any_captures = true;
                    match &cap.kind {
                        CaptureKind::Indent | CaptureKind::IndentAlways => {
                            indent_delta += 1;
                        }
                        CaptureKind::Outdent | CaptureKind::OutdentAlways => {
                            indent_delta -= 1;
                        }
                        CaptureKind::Align { anchor_col } => {
                            if align_col.is_none() {
                                align_col = Some(*anchor_col);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }

    if let Some(col) = align_col {
        return " ".repeat(col);
    }

    let tab_str = buf.indent_style.tab_string();

    if !any_captures {
        fallback_indent.to_string()
    } else if indent_delta > 0 {
        tab_str.repeat(indent_delta as usize)
    } else {
        String::new()
    }
}

fn collect_indent_captures(
    state: &TreeSitterState,
    rope: &ropey::Rope,
    query: Arc<Query>,
    injected: &HashMap<String, Arc<Query>>,
    query_end_byte: usize,
) -> HashMap<usize, Vec<IndentCapture>> {
    let mut result: HashMap<usize, Vec<IndentCapture>> = HashMap::new();

    let mut walker = QueryWalkerBuilder::new(state, rope, query)
        .with_injected_queries(injected.clone())
        .build();

    walker.walk(|entry| {
        if !check_predicates(&entry) {
            return true;
        }

        let pattern_idx = entry.query_match.pattern_index;

        let pattern_scope = {
            let props = entry.query.property_settings(pattern_idx);
            props
                .iter()
                .find(|p| p.key.as_ref() == "scope")
                .and_then(|p| p.value.as_ref().map(|v| v.as_ref().to_string()))
                .map(|v| if v == "all" { Scope::All } else { Scope::Tail })
                .unwrap_or(Scope::Tail)
        };

        let anchor_col = {
            let anchor_capture_idx = entry
                .query
                .capture_names()
                .iter()
                .position(|n| *n == "anchor");
            anchor_capture_idx.and_then(|idx| {
                entry
                    .query_match
                    .captures
                    .iter()
                    .find(|c| c.index as usize == idx)
                    .map(|c| c.node.start_position().column)
            })
        };

        for capture in entry.query_match.captures {
            let node = capture.node;
            let node_start = node.start_byte() + entry.byte_offset;

            if node_start >= query_end_byte {
                continue;
            }

            let capture_name = entry.query.capture_names()[capture.index as usize];
            let node_id = node.id();

            let kind = match capture_name {
                "indent" | "indent.begin" => CaptureKind::Indent,
                "indent.always" => CaptureKind::IndentAlways,
                "outdent" | "outdent.begin" => CaptureKind::Outdent,
                "outdent.always" => CaptureKind::OutdentAlways,
                "extend" => CaptureKind::Extend,
                "extend.prevent-once" => CaptureKind::ExtendPreventOnce,
                "align" => CaptureKind::Align {
                    anchor_col: anchor_col.unwrap_or(0),
                },
                _ => continue,
            };

            let effective_scope = match kind {
                CaptureKind::Outdent | CaptureKind::OutdentAlways | CaptureKind::Align { .. } => {
                    Scope::All
                }
                _ => pattern_scope.clone(),
            };

            result.entry(node_id).or_default().push(IndentCapture {
                kind,
                scope: effective_scope,
            });
        }

        true
    });

    result
}

fn is_first_in_line(node: tree_sitter::Node, buf: &TextBuffer) -> bool {
    let col = node.start_position().column;
    if col == 0 {
        return true;
    }
    let line_idx = node.start_position().row;
    buf.line(line_idx)
        .map(|l| l.chars().take(col).all(|c| c == ' ' || c == '\t'))
        .unwrap_or(true)
}

fn check_predicates(entry: &QueryMatchEntry) -> bool {
    let query = &entry.query;
    let pattern_idx = entry.query_match.pattern_index;

    for predicate in query.general_predicates(pattern_idx) {
        if predicate.operator.as_ref() == "not-same-line?" && !check_not_same_line(predicate, entry)
        {
            return false;
        }
    }
    true
}

fn check_not_same_line(predicate: &QueryPredicate, entry: &QueryMatchEntry) -> bool {
    let mut lines = Vec::new();

    for arg in &predicate.args {
        if let QueryPredicateArg::Capture(idx) = arg {
            for cap in entry.query_match.captures {
                if cap.index == *idx {
                    lines.push(cap.node.start_position().row);
                }
            }
        }
    }

    if lines.len() < 2 {
        return true;
    }

    let first = lines[0];
    for &line in &lines[1..] {
        if line == first {
            return false;
        }
    }

    true
}
