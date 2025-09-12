use kerbin_core::TextBuffer;
use ropey::Rope;
use tree_sitter::{Query, QueryCursor, QueryMatch, StreamingIterator, Tree};

use crate::{GrammarManager, TSState, TextProviderRope, TreeSitterStates};

/// Represents the active Tree-sitter context at a specific byte position.
pub struct ActiveTreeSitterContext<'a> {
    pub tree: &'a Tree,
    pub language_name: &'a str,
    pub buffer_rope: &'a Rope,
}

/// Helper function to get the active Tree-sitter tree and its language
/// at a specific byte position, considering injected languages.
pub fn get_tree_and_language_at_byte<'a>(
    buffer_rope: &'a Rope,
    cursor_byte: usize,
    ts_state: &'a TSState,
) -> Option<ActiveTreeSitterContext<'a>> {
    if let Some((lang_name, (_, tree_opt))) =
        ts_state.injected_parsers.iter().find(|(_, (_, tree_opt))| {
            if let Some(tree) = tree_opt {
                let range = tree.root_node().byte_range();
                range.start <= cursor_byte && cursor_byte <= range.end
            } else {
                false
            }
        })
        && let Some(tree) = tree_opt.as_ref()
    {
        return Some(ActiveTreeSitterContext {
            tree,
            language_name: lang_name,
            buffer_rope,
        });
    }

    if let Some(tree) = ts_state.primary_tree.as_ref() {
        return Some(ActiveTreeSitterContext {
            tree,
            language_name: &ts_state.language_name,
            buffer_rope,
        });
    }

    None
}

/// Runs a Tree-sitter query on the relevant tree at the current cursor position
/// and applies a callback function to each match.
pub fn match_each<F>(
    buffer: &TextBuffer,
    ts_states: &TreeSitterStates,
    grammars: &mut GrammarManager,

    cursor_byte: usize,
    query_name: &str,
    mut f: F,
) where
    F: FnMut(&QueryMatch, &Query),
{
    let buffer_path = &buffer.path;
    let buffer_rope = &buffer.rope;

    if let Some(Some(ts_state)) = ts_states.bufs.get(buffer_path)
        && let Some(context) = get_tree_and_language_at_byte(buffer_rope, cursor_byte, ts_state)
        && let Some(query) = grammars.get_query(context.language_name, query_name)
    {
        let mut query_cursor = QueryCursor::new();
        let provider = TextProviderRope(context.buffer_rope);
        let mut matches = query_cursor.matches(&query, context.tree.root_node(), &provider);

        while let Some(m) = matches.next() {
            f(m, &query);
        }
    }
}
