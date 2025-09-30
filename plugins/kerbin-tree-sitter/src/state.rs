use kerbin_core::{ascii_forge::prelude::*, *};
use kerbin_macros::State;

use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
};

use ropey::Rope;
use tree_sitter::{
    InputEdit, Parser, Query, QueryCapture, QueryCursor, StreamingIterator, TextProvider, Tree,
};

use crate::GrammarManager;

#[derive(State, Default)]
pub struct HighlightMap(pub BTreeMap<String, BTreeMap<usize, ContentStyle>>);

pub struct TSState {
    pub parser: Parser,
    pub primary_tree: Option<Tree>,
    pub injected_parsers: HashMap<String, (Parser, Option<Tree>)>,
    pub tree_sitter_dirty: bool,
    pub changes: Vec<InputEdit>,
    pub language_name: String,
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
            primary_tree: None,
            injected_parsers: HashMap::new(),
            tree_sitter_dirty: true,
            changes: vec![],
            language_name: String::new(),
        }
    }

    pub fn init(ext: &str, grammar_manager: &mut GrammarManager) -> Option<Self> {
        let mut s;
        let lang_name = grammar_manager.extension_map.get(ext).cloned();

        if let Some(language) = grammar_manager.get_language_for_ext(ext) {
            s = Self::new();
            s.parser.set_language(&language).unwrap();
            s.language_name = lang_name.unwrap_or_default();
        } else {
            return None;
        }

        Some(s)
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

pub fn map_query<'a: 'b + 'c, 'b, 'c, T, F>(
    tree: &'a Tree,
    query: &'b Query,
    text: &'c Rope,
    mut mapper: F,
) -> Vec<T>
where
    F: FnMut(QueryCapture<'b>, &str) -> Option<T>,
{
    let mut query_cursor = QueryCursor::new();
    let provider = TextProviderRope(text);
    let mut matches = query_cursor.matches(query, tree.root_node(), &provider);
    let mut results = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            if let Some(mapped_value) = mapper(*capture, capture_name) {
                results.push(mapped_value);
            }
        }
    }
    results
}

pub fn highlight(
    text: &Rope,
    tree: &Tree,
    query: &Query,
    theme: &Theme,
) -> BTreeMap<usize, ContentStyle> {
    let mut highlights = map_query(tree, query, text, |capture, capture_name| {
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
            Some(Highlight {
                range: capture.node.byte_range(),
                style,
            })
        } else {
            tracing::debug!("No query match found for: {capture_name}");
            None
        }
    });

    if highlights.is_empty() {
        let mut map = BTreeMap::new();
        map.insert(0, ContentStyle::default());
        return map;
    }

    highlights.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then_with(|| b.range.end.cmp(&a.range.end))
    });

    let mut result_map = BTreeMap::new();
    let mut style_stack: Vec<(usize, ContentStyle)> = vec![(usize::MAX, ContentStyle::default())];
    let mut last_pos = 0;

    for highlight in highlights {
        let start = highlight.range.start;
        let end = highlight.range.end;

        while style_stack.last().unwrap().0 <= start {
            let (expiry_pos, _) = style_stack.pop().unwrap();
            let current_style = style_stack.last().unwrap().1;

            if expiry_pos > last_pos {
                result_map.insert(expiry_pos, current_style);
                last_pos = expiry_pos;
            }
        }

        let current_style = style_stack.last().unwrap().1;
        if highlight.style != current_style {
            result_map.insert(start, highlight.style);
            last_pos = start;
        }

        style_stack.push((end, highlight.style));
        style_stack.sort_by(|a, b| b.0.cmp(&a.0));
    }

    while style_stack.len() > 1 {
        let (expiry_pos, _) = style_stack.pop().unwrap();
        let current_style = style_stack.last().unwrap().1;
        if expiry_pos > last_pos {
            result_map.insert(expiry_pos, current_style);
            last_pos = expiry_pos;
        }
    }

    let mut final_map = BTreeMap::new();
    let mut last_style: Option<ContentStyle> = None;
    for (pos, style) in result_map {
        if Some(style) != last_style {
            final_map.insert(pos, style);
            last_style = Some(style);
        }
    }

    final_map
}

pub async fn render_tree_sitter_extmarks(bufs: Res<Buffers>, highlights: Res<HighlightMap>) {
    get!(bufs, highlights);

    let buf_arc = bufs.cur_buffer();
    let mut buf = buf_arc.write().unwrap();

    buf.renderer.clear_extmark_ns("tree-sitter::highlight");

    let Some(hl_map) = highlights.0.get(&buf.path) else {
        return;
    };

    let mut last_pos: Option<usize> = None;
    let mut last_hl: Option<ContentStyle> = None;

    for (&pos, &style) in hl_map.iter() {
        if let (Some(start), Some(prev_style)) = (last_pos, last_hl)
            && pos > start
        {
            buf.renderer.add_extmark_range(
                "tree-sitter::highlight",
                start..pos,
                0,
                vec![ExtmarkDecoration::Highlight { hl: prev_style }],
            );
        }
        last_pos = Some(pos);
        last_hl = Some(style);
    }

    if let (Some(start), Some(prev_style)) = (last_pos, last_hl) {
        let len = buf.rope.len().saturating_sub(start);
        if len > 0 {
            buf.renderer.add_extmark_range(
                "tree-sitter::highlight",
                start..(start + len),
                0,
                vec![ExtmarkDecoration::Highlight { hl: prev_style }],
            );
        }
    }
}
