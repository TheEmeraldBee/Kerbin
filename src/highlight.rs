use ascii_forge::{
    prelude::Color,
    window::{ContentStyle, Stylize},
};
use std::collections::{BTreeMap, HashMap};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

/// Maps Tree-sitter capture names (e.g., "keyword") to a style.
pub struct HighlightConfiguration {
    pub captures: HashMap<String, ContentStyle>,
}

impl Default for HighlightConfiguration {
    fn default() -> Self {
        let mut captures = HashMap::new();
        let blue = Color::Rgb {
            r: 97,
            g: 175,
            b: 239,
        };
        let purple = Color::Rgb {
            r: 198,
            g: 120,
            b: 221,
        };
        let green = Color::Rgb {
            r: 152,
            g: 195,
            b: 121,
        };
        let grey = Color::Rgb {
            r: 92,
            g: 99,
            b: 112,
        };
        let yellow = Color::Rgb {
            r: 229,
            g: 192,
            b: 123,
        };
        let orange = Color::Rgb {
            r: 209,
            g: 154,
            b: 102,
        };
        let red = Color::Rgb {
            r: 224,
            g: 108,
            b: 117,
        };

        // Standard captures from common highlight queries
        captures.insert("keyword".into(), ContentStyle::new().with(purple));
        captures.insert("function".into(), ContentStyle::new().with(blue));
        captures.insert("function.builtin".into(), ContentStyle::new().with(blue));
        captures.insert("function.macro".into(), ContentStyle::new().with(blue));
        captures.insert("string".into(), ContentStyle::new().with(green));
        captures.insert("comment".into(), ContentStyle::new().with(grey).italic());
        captures.insert("type".into(), ContentStyle::new().with(yellow));
        captures.insert("type.builtin".into(), ContentStyle::new().with(yellow));
        captures.insert("constant".into(), ContentStyle::new().with(orange));
        captures.insert("constant.builtin".into(), ContentStyle::new().with(orange));
        captures.insert("variable".into(), ContentStyle::new().with(red));
        captures.insert("variable.parameter".into(), ContentStyle::new().with(red));
        captures.insert("property".into(), ContentStyle::new().with(red));
        captures.insert("punctuation.bracket".into(), ContentStyle::new().with(grey));
        captures.insert(
            "punctuation.delimiter".into(),
            ContentStyle::new().with(grey),
        );
        captures.insert("operator".into(), ContentStyle::new().with(purple));

        Self { captures }
    }
}

/// Traverses a tree using a query and returns a list of styled ranges.
pub fn highlight(
    text: &[String],
    tree: &Tree,
    query: &Query,
    config: &HighlightConfiguration,
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
            if let Some(style) = config.captures.get(&capture_name.to_string()) {
                let range = capture.node.byte_range();

                // Only take the first style we find
                highlight_map.entry(range.start).or_insert(*style);

                highlight_map
                    .entry(range.end)
                    .or_insert(ContentStyle::default());
            }
        }
    }

    highlight_map
}
