use std::{collections::BTreeMap, ops::Range, sync::Arc};

use ascii_forge::window::ContentStyle;
use ropey::Rope;
use tree_sitter::{InputEdit, Parser, Query, QueryCursor, StreamingIterator, TextProvider, Tree};

use crate::{GrammarManager, Theme};

pub struct TSState {
    pub parser: Parser,

    pub tree: Option<Tree>,

    pub query: Option<Arc<Query>>,

    pub highlights: BTreeMap<usize, ContentStyle>,

    pub tree_sitter_dirty: bool,
    pub changes: Vec<InputEdit>,
}

impl Default for TSState {
    fn default() -> Self {
        Self::new()
    }
}

impl TSState {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),

            tree: None,

            query: None,

            highlights: BTreeMap::new(),

            tree_sitter_dirty: false,
            changes: vec![],
        }
    }

    pub fn init(
        ext: &str,
        text: &Rope,
        grammar_manager: &mut GrammarManager,
        theme: &Theme,
    ) -> Option<Self> {
        let mut s;

        if let Some((language, q)) = grammar_manager.get_language_and_query_for_ext(ext) {
            s = Self::new();
            s.parser.set_language(&language).unwrap();

            s.query = q
        } else {
            return None;
        }

        let tree = s.parser.parse_with_options(
            &mut |byte, _point| {
                let res = text.get_chunk(byte);
                res.map(|x| &x.0[(byte - x.1)..]).unwrap_or_default()
            },
            None,
            None,
        );

        if let (Some(q), Some(t)) = (s.query.as_ref(), tree.as_ref()) {
            s.highlights = highlight(text, t, q, theme);
        }

        Some(s)
    }

    pub fn update_tree_and_highlights(&mut self, text: &Rope, theme: &Theme) {
        if !self.tree_sitter_dirty {
            return;
        }

        tracing::warn!("Running tree update");

        if let Some(tree) = &mut self.tree {
            for edit in &self.changes {
                tree.edit(edit);
            }
        }

        self.tree = self.parser.parse_with_options(
            &mut |byte, _point| {
                let res = text.chunk(byte).0;
                tracing::info!("{}", res);
                res
            },
            self.tree.as_ref(),
            None,
        );

        if let (Some(t), Some(q)) = (self.tree.as_ref(), self.query.as_ref()) {
            self.highlights = highlight(text, t, q, theme);
        }

        self.tree_sitter_dirty = false;
        self.changes.clear();
    }
}

#[derive(Debug)]
struct Highlight {
    range: Range<usize>,
    style: ContentStyle,
}

pub struct TextProviderRope<'a>(pub &'a Rope);

impl<'a> TextProvider<&'a [u8]> for &'a TextProviderRope<'a> {
    type I = ChunksBytes<'a>;
    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        let mut byte_range = node.byte_range();

        if self.0.len() <= byte_range.start {
            return ChunksBytes(None);
        }

        byte_range.end = byte_range.end.min(self.0.len());

        ChunksBytes(Some(self.0.slice(byte_range).chunks()))
    }
}

pub struct ChunksBytes<'a>(Option<ropey::iter::Chunks<'a>>);

impl<'a> Iterator for ChunksBytes<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        self.0.as_mut()?.next().map(|s| s.as_bytes())
    }
}

pub fn highlight(
    text: &Rope,
    tree: &Tree,
    query: &Query,
    theme: &Theme,
) -> BTreeMap<usize, ContentStyle> {
    let mut query_cursor = QueryCursor::new();

    let provider = TextProviderRope(text);
    let mut matches = query_cursor.matches(query, tree.root_node(), &provider);
    let mut highlights: Vec<Highlight> = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = &query.capture_names()[capture.index as usize];

            let mut components: Vec<&str> = capture_name.split('.').collect();
            let mut found_style: Option<ContentStyle> = None;

            while !components.is_empty() {
                let current_name = components.join(".");
                if let Some(style) = theme.get(&format!("ts.{}", current_name)) {
                    found_style = Some(style);
                    break;
                }
                components.pop();
            }

            if let Some(style) = found_style {
                highlights.push(Highlight {
                    range: capture.node.byte_range(),
                    style,
                });
            }
        }
    }

    highlights.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then_with(|| b.range.end.cmp(&a.range.end))
    });

    let mut highlight_map = BTreeMap::new();
    let mut last_pos = 0;

    for h in highlights {
        if h.range.start > last_pos {
            highlight_map
                .entry(last_pos)
                .or_insert(ContentStyle::default());
        }

        let style_at_end = highlight_map
            .range(..=h.range.end)
            .next_back()
            .map(|(_, &style)| style)
            .unwrap_or_default();

        highlight_map.insert(h.range.start, h.style);
        highlight_map.insert(h.range.end, style_at_end);

        last_pos = h.range.end;
    }

    highlight_map.entry(0).or_insert(ContentStyle::default());

    let mut final_map = BTreeMap::new();
    let mut last_style: Option<ContentStyle> = None;
    for (pos, style) in highlight_map {
        if Some(style) != last_style {
            final_map.insert(pos, style);
            last_style = Some(style);
        }
    }

    final_map
}
