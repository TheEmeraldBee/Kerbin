use kerbin_core::{kerbin_macros::Command, *};

use crate::{
    grammar_manager::GrammarManager, query_walker::QueryWalkerBuilder, state::TreeSitterState,
};

#[derive(Command)]
pub enum ScopeInfoCommand {
    /// Shows tree-sitter capture information at cursor position
    #[command]
    TreeSitterScopeInfo,
}

#[async_trait::async_trait]
impl Command for ScopeInfoCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::TreeSitterScopeInfo => {
                tree_sitter_scope_info(state).await;
            }
        }
        false
    }
}

async fn tree_sitter_scope_info(state: &mut State) {
    let mut buffers = state.lock_state::<Buffers>().await;
    let mut grammars = state.lock_state::<GrammarManager>().await;
    let config_path = state.lock_state::<ConfigFolder>().await.0.clone();
    let log = state.lock_state::<LogSender>().await.clone();

    let buf = buffers.cur_buffer_mut().await;

    let Some(ts_state) = buf.get_state::<TreeSitterState>().await else {
        log.low(
            "tree-sitter::scope_info",
            "No tree-sitter state available for this buffer",
        );
        return;
    };

    // Get cursor position
    let cursor_pos = buf.primary_cursor().get_cursor_byte();

    // Get the highlights query
    let Some((highlights_query, injected_queries)) =
        grammars.get_query_set(&config_path, "highlights", &ts_state)
    else {
        log.low(
            "tree-sitter::scope_info",
            "No highlights query available for this language",
        );
        return;
    };

    // Collect all captures at cursor position
    let mut captures_at_cursor: Vec<CaptureInfo> = Vec::new();

    // Create a walker with the highlights query
    let mut walker = QueryWalkerBuilder::new(&ts_state, &buf.rope, highlights_query)
        .with_injected_queries(injected_queries)
        .build();

    walker.walk(|entry| {
        for capture in entry.query_match.captures {
            let node_range = capture.node.byte_range();
            let adjusted_range =
                (node_range.start + entry.byte_offset)..(node_range.end + entry.byte_offset);

            // Check if cursor is within this node
            if cursor_pos >= adjusted_range.start && cursor_pos < adjusted_range.end {
                let capture_name = entry.query.capture_names()[capture.index as usize];

                // Get node information
                let node_kind = capture.node.kind();
                let node_text = buf
                    .rope
                    .slice(adjusted_range.start..adjusted_range.end)
                    .to_string();

                // Truncate text if too long
                let display_text = if node_text.len() > 50 {
                    format!("{}...", &node_text[..47])
                } else {
                    node_text
                };

                captures_at_cursor.push(CaptureInfo {
                    capture_name: capture_name.to_string(),
                    node_kind: node_kind.to_string(),
                    node_text: display_text,
                    lang: entry.lang.clone(),
                    is_injected: entry.is_injected,
                    specificity: capture_name.matches('.').count(),
                });
            }
        }
        true
    });

    if captures_at_cursor.is_empty() {
        log.low("tree-sitter::scope_info", "No captures found at cursor");
        return;
    }

    // Sort by specificity (most specific first) and then by capture name
    captures_at_cursor.sort_by(|a, b| {
        b.specificity
            .cmp(&a.specificity)
            .then_with(|| a.capture_name.cmp(&b.capture_name))
    });

    // Build the output message
    let mut output = "Tree-Sitter Scope Info at cursor:\n\n".to_string();

    // Show tree structure information
    let primary_capture = &captures_at_cursor[0];
    output.push_str(&format!(
        "Primary Language: {}\n",
        if primary_capture.is_injected {
            format!("{} (injected)", primary_capture.lang)
        } else {
            primary_capture.lang.clone()
        }
    ));
    output.push_str(&format!("Node Type: {}\n", primary_capture.node_kind));
    output.push_str(&format!("Node Text: \"{}\"\n\n", primary_capture.node_text));

    // Show all captures in order of specificity
    output.push_str("Captures (most specific first):\n");
    for (idx, capture) in captures_at_cursor.iter().enumerate() {
        let lang_marker = if capture.is_injected {
            format!(" [{}]", capture.lang)
        } else {
            String::new()
        };

        output.push_str(&format!(
            "  {}. @{}{}\n",
            idx + 1,
            capture.capture_name,
            lang_marker
        ));
    }

    // Show syntax tree path (parent hierarchy)
    if let Some(tree) = &ts_state.tree {
        output.push_str("\nSyntax Tree Path:\n");
        let mut node = tree
            .root_node()
            .descendant_for_byte_range(cursor_pos, cursor_pos);

        let mut depth = 0;
        while let Some(current_node) = node {
            let indent = "  ".repeat(depth);
            let node_kind = current_node.kind();
            let is_named = if current_node.is_named() {
                ""
            } else {
                " (anonymous)"
            };

            output.push_str(&format!("{}{}{}\n", indent, node_kind, is_named));

            node = current_node.parent();
            depth += 1;

            // Limit depth to prevent excessive output
            if depth > 15 {
                output.push_str(&format!("{}...\n", "  ".repeat(depth)));
                break;
            }
        }
    }

    log.low("tree-sitter::scope_info", output);
}

#[derive(Debug, Clone)]
struct CaptureInfo {
    capture_name: String,
    node_kind: String,
    node_text: String,
    lang: String,
    is_injected: bool,
    specificity: usize,
}
