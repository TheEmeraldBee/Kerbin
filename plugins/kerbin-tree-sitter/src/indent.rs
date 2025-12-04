use kerbin_core::{buffer::SafeRopeAccess, *};
use std::sync::Arc;
use tree_sitter::{Query, QueryPredicate, QueryPredicateArg};

use crate::{
    grammar_manager::GrammarManager,
    query_walker::{QueryMatchEntry, QueryWalkerBuilder},
    state::TreeSitterState,
};

#[derive(Command)]
pub enum IndentCommand {
    #[command(drop_ident, name = "tree_sitter_newline", name = "ts_nl")]
    /// Inserts a newline and uses tree-sitter to calculate indentation
    Newline,
}

#[async_trait::async_trait]
impl Command for IndentCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Newline => {
                newline_and_indent(state).await;
            }
        }
        false
    }
}

async fn newline_and_indent(state: &mut State) {
    let mut buffers = state.lock_state::<Buffers>().await;
    let mut grammars = state.lock_state::<GrammarManager>().await;
    let config_path = state.lock_state::<ConfigFolder>().await.0.clone();

    let mut buf = buffers.cur_buffer_mut().await;

    let cursor_byte = buf.primary_cursor().get_cursor_byte();
    let current_line_idx = buf.byte_to_line_clamped(cursor_byte);

    let current_line_indent = get_line_indent(&buf, current_line_idx);

    buf.action(Insert {
        byte: cursor_byte,
        content: "\n".to_string(),
    });
    buf.move_bytes(1, false);

    // 3. Calculate Indent
    let Some(ts_state) = buf.get_state_mut::<TreeSitterState>().await else {
        // Fallback: copy previous line indent
        if !current_line_indent.is_empty() {
            buf.action(Insert {
                byte: cursor_byte + 1,
                content: current_line_indent.clone(),
            });
            buf.move_bytes(current_line_indent.len() as isize, false);
        }
        return;
    };

    // Load indent query
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
        current_line_idx,
        &current_line_indent,
    );

    // 4. Insert Indentation
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

fn calculate_indent(
    state: &TreeSitterState,
    buf: &TextBuffer,
    query: Arc<Query>,
    injected: std::collections::HashMap<String, Arc<Query>>,
    cursor_byte: usize,
    _current_line_idx: usize,
    fallback_indent: &str,
) -> String {
    let mut walker = QueryWalkerBuilder::new(state, buf.get_rope(), query)
        .with_injected_queries(injected)
        .build();

    let mut deepest_capture: Option<(usize, usize, String)> = None; // (depth, start_line, indent_type)

    walker.walk(|entry| {
        // Check predicates first
        if !check_predicates(&entry) {
            return true;
        }

        for capture in entry.query_match.captures {
            let node = capture.node;
            let range = node.byte_range();
            let start = range.start + entry.byte_offset;
            let end = range.end + entry.byte_offset;

            if cursor_byte >= start && cursor_byte <= end {
                let capture_name = entry.query.capture_names()[capture.index as usize];

                if capture_name == "indent"
                    || capture_name == "indent.begin"
                    || capture_name == "indent.always"
                {
                    if let Some((current_depth, _, _)) = deepest_capture {
                        if start > current_depth {
                            deepest_capture =
                                Some((start, node.start_position().row, "indent".to_string()));
                        }
                    } else {
                        deepest_capture =
                            Some((start, node.start_position().row, "indent".to_string()));
                    }
                }
            }
        }
        true
    });

    if let Some((_, start_line, _)) = deepest_capture {
        let base_indent = get_line_indent(buf, start_line);
        let indent_unit = "    ";
        return format!("{}{}", base_indent, indent_unit);
    }

    fallback_indent.to_string()
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
            // Find the node for this capture in the current match
            for cap in entry.query_match.captures {
                if cap.index == *idx {
                    lines.push(cap.node.start_position().row);
                }
            }
        }
    }

    if lines.len() < 2 {
        return true; // Not enough args to compare
    }

    let first = lines[0];
    for &line in &lines[1..] {
        if line == first {
            return false; // Found two on same line
        }
    }

    true
}
