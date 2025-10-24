use std::collections::{HashMap, HashSet};

use kerbin_core::{kerbin_macros::Command, *};

use crate::TreeSitterStates;
use crate::{GrammarManager, match_each};

fn get_line_indent(buffer: &TextBuffer, line_idx: usize) -> usize {
    if line_idx >= buffer.rope.len_lines(LineType::LF_CR) {
        return 0;
    }
    let line_text = buffer.rope.line(line_idx, LineType::LF_CR).to_string();
    line_text.len() - line_text.trim_start().len()
}

fn get_node_at_position<'a>(
    buffer: &'a TextBuffer,
    ts_states: &'a TreeSitterStates,
    line_idx: usize,
    col: usize,
) -> Option<tree_sitter::Node<'a>> {
    let byte_pos = buffer.rope.line_to_byte_idx(line_idx, LineType::LF_CR) + col;

    if let Some(Some(ts_state)) = ts_states.bufs.get(&buffer.path)
        && let Some(tree) = &ts_state.primary_tree
    {
        let root = tree.root_node();
        return Some(
            root.descendant_for_byte_range(byte_pos, byte_pos + 1)
                .unwrap_or(root),
        );
    }
    None
}

fn collect_indent_captures(
    buffer: &TextBuffer,
    ts_states: &TreeSitterStates,
    grammars: &mut GrammarManager,
) -> HashMap<String, HashMap<usize, HashMap<String, String>>> {
    let mut captures = HashMap::new();
    let capture_types = vec![
        "indent.auto",
        "indent.begin",
        "indent.end",
        "indent.dedent",
        "indent.branch",
        "indent.ignore",
        "indent.align",
        "indent.zero",
    ];

    for capture_type in capture_types {
        captures.insert(capture_type.to_string(), HashMap::new());
    }

    match_each(buffer, ts_states, grammars, 0, "indent", |m, query| {
        for cap in m.captures {
            let name = query.capture_names()[cap.index as usize];
            let node = cap.node;
            let node_id = node.id();

            if name.starts_with("indent.") {
                captures
                    .entry(name.to_string())
                    .or_default()
                    .insert(node_id, HashMap::new());
            }
        }
    });

    captures
}

fn calculate_indent(
    buffer: &TextBuffer,
    ts_states: &TreeSitterStates,
    grammars: &mut GrammarManager,
    target_line: usize,
    cursor_col: Option<usize>,
) -> Option<i32> {
    let captures = collect_indent_captures(buffer, ts_states, grammars);

    // Get the node at the cursor position on the target line
    let col = cursor_col.unwrap_or_else(|| {
        let line_text = buffer.rope.line(target_line, LineType::LF_CR).to_string();
        let indent_cols = get_line_indent(buffer, target_line);
        let trimmed_len = line_text.trim().len();
        indent_cols + trimmed_len.saturating_sub(1).max(0)
    });

    let cursor_byte = buffer.rope.line_to_byte_idx(target_line, LineType::LF_CR) + col;

    let mut node = get_node_at_position(buffer, ts_states, target_line, col)?;

    let mut indent = 0;
    let mut processed_rows = HashSet::new();

    if let Some(zero_nodes) = captures.get("indent.zero")
        && zero_nodes.contains_key(&node.id())
    {
        return Some(0);
    }

    while let Some(current_node) = Some(node) {
        let node_id = current_node.id();
        let start_row = current_node.start_position().row;
        let end_row = current_node.end_position().row;
        let end_byte = current_node.end_byte();

        if let Some(ignore_nodes) = captures.get("indent.auto")
            && ignore_nodes.contains_key(&node_id)
            && start_row < target_line
            && target_line <= end_row
        {
            return Some(i32::MAX);
        }

        if let Some(ignore_nodes) = captures.get("indent.ignore")
            && ignore_nodes.contains_key(&node_id)
            && start_row < target_line
            && target_line <= end_row
        {
            return Some(0);
        }

        let mut is_processed = false;

        if !processed_rows.contains(&start_row) {
            if let Some(branch_nodes) = captures.get("indent.branch")
                && branch_nodes.contains_key(&node_id)
                && start_row == target_line
            {
                indent -= 1;
                is_processed = true;
            }

            if let Some(dedent_nodes) = captures.get("indent.dedent")
                && dedent_nodes.contains_key(&node_id)
                && start_row != target_line
            {
                indent -= 1;
                is_processed = true;
            }
        }

        if !processed_rows.contains(&start_row)
            && let Some(begin_nodes) = captures.get("indent.begin")
            && begin_nodes.contains_key(&node_id)
            && (start_row != end_row)
        {
            // For begin nodes, check if:
            // 1. The node starts before the target line - always indent
            // 2. The node starts on the target line, extends beyond, AND cursor is within the node's range
            if start_row < target_line {
                indent += 1;
                is_processed = true;
            } else if start_row == target_line && end_row > target_line && cursor_byte < end_byte {
                // Only indent if cursor is still inside this begin node
                // This prevents parameters from affecting indent after their closing paren
                indent += 1;
                is_processed = true;
            }
        }

        if is_processed {
            processed_rows.insert(start_row);
        }

        node = match current_node.parent() {
            Some(parent) => parent,
            None => break,
        };
    }

    Some(indent.max(0))
}

#[derive(Command, Debug, Clone)]
pub enum TSCommand {
    #[command(drop_ident, name = "ts_newline", name = "ts_nl")]
    /// Inserts a newline at the cursor position,
    /// using tree-sitter to define the newline indentation
    Newline(#[command(type_name = "?bool", name = "extend")] Option<bool>),
}

#[async_trait::async_trait]
impl Command for TSCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Newline(extend) => {
                let buffers = state.lock_state::<Buffers>().await;
                let ts_states = state.lock_state::<TreeSitterStates>().await;
                let mut grammars = state.lock_state::<GrammarManager>().await;
                let plugin_config = state.lock_state::<PluginConfig>().await;

                let buffer = buffers.cur_buffer().await;

                let cursor_byte = buffer.primary_cursor().get_cursor_byte();
                let buffer_ext = buffer.ext.clone();
                let current_line_idx = buffer.rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);

                // Calculate cursor column position on current line
                let line_start_byte = buffer
                    .rope
                    .line_to_byte_idx(current_line_idx, LineType::LF_CR);
                let cursor_col = cursor_byte.saturating_sub(line_start_byte);

                let indent_width = plugin_config
                    .0
                    .get("tree-sitter")
                    .and_then(|ts_config| ts_config.get("indent"))
                    .and_then(|indent_config| indent_config.get(&buffer_ext))
                    .and_then(|indent_val| indent_val.as_integer())
                    .unwrap_or(4) as usize;

                // Calculate indent based on current position (before newline)
                let indent_amount = calculate_indent(
                    &buffer,
                    &ts_states,
                    &mut grammars,
                    current_line_idx,
                    Some(cursor_col),
                )
                .unwrap_or(0)
                .max(0) as usize;

                let new_indent_str = if indent_amount == i32::MAX as usize {
                    // i32::MAX is reserved for auto. This then gets the chars that are considered
                    // whitespace to clone the start of the line :)
                    let mut res = String::new();
                    for chr in buffer.rope.line(current_line_idx, LineType::LF_CR).chars() {
                        match chr {
                            ' ' => res.push(' '),
                            '\t' => res.push('\t'),

                            // Not whitespace
                            _ => break,
                        }
                    }

                    res
                } else {
                    " ".repeat(indent_amount * indent_width)
                };

                state
                    .lock_state::<CommandSender>()
                    .await
                    .send(Box::new(BufferCommand::Append(
                        format!("\n{}", new_indent_str),
                        extend.unwrap_or(false),
                    )))
                    .unwrap();

                true
            }
        }
    }
}
