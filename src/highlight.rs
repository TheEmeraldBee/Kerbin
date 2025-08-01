use ascii_forge::window::ContentStyle;
use std::collections::BTreeMap;
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use crate::Theme;

/// Traverses a tree using a query and returns a list of styled ranges.
pub fn highlight(
    text: &[String],
    tree: &Tree,
    query: &Query,
    theme: &Theme,
) -> BTreeMap<usize, ContentStyle> {
    let mut highlight_map = BTreeMap::new();
    let mut query_cursor = QueryCursor::new();

    let joined = text.join("\n");

    // The `matches` method executes the query and gives us an iterator of all captures.
    let mut matches = query_cursor.matches(query, tree.root_node(), joined.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            #[allow(clippy::unnecessary_to_owned)]
            if let Some(style) = theme
                .get(&format!("ts.{capture_name}"))
                .map(|x| x.to_content_style())
            {
                let range = capture.node.byte_range();

                // Only take the first style we find
                highlight_map.entry(range.start).or_insert(style);

                highlight_map
                    .entry(range.end)
                    .or_insert(ContentStyle::default());
            }
        }
    }

    highlight_map
}
