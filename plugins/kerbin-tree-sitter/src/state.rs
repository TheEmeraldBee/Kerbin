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

#[derive(Debug, Clone)]
struct Highlight {
    range: Range<usize>,
    style: ContentStyle,
    depth: usize,
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

fn calculate_node_depth(node: tree_sitter::Node) -> usize {
    let mut depth = 0;
    let mut current = node;
    while let Some(parent) = current.parent() {
        depth += 1;
        current = parent;
    }
    depth
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
                depth: calculate_node_depth(capture.node),
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

    // Sort highlights: by start position, then by depth (deeper = higher priority)
    highlights.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then_with(|| a.depth.cmp(&b.depth))
    });

    // Build a map of byte position -> style layers
    // Each position can have multiple layers, we'll resolve them at the end
    let mut layers: BTreeMap<usize, Vec<(usize, ContentStyle, Range<usize>)>> = BTreeMap::new();

    for hl in &highlights {
        // Add start point
        layers
            .entry(hl.range.start)
            .or_default()
            .push((hl.depth, hl.style, hl.range.clone()));

        // Add end point
        layers.entry(hl.range.end).or_default();
    }

    // Now walk through all positions and resolve the active style at each point
    let mut result = BTreeMap::new();
    let mut active_layers: Vec<(usize, ContentStyle, Range<usize>)> = vec![];
    let default_style = ContentStyle::default();

    for (&pos, new_layers) in &layers {
        // Remove expired layers
        active_layers.retain(|(_, _, range)| range.end > pos);

        // Add new layers
        active_layers.extend(new_layers.iter().cloned());

        // Sort by depth (deeper = higher priority = later in list)
        active_layers.sort_by_key(|(depth, _, _)| *depth);

        // The style at this position is the deepest active layer
        let current_style = active_layers
            .last()
            .map(|(_, style, _)| *style)
            .unwrap_or(default_style);

        result.insert(pos, current_style);
    }

    // Ensure we start at position 0
    if !result.contains_key(&0) {
        result.insert(0, default_style);
    }

    // Remove consecutive duplicate styles
    let mut final_result = BTreeMap::new();
    let mut last_style: Option<ContentStyle> = None;

    for (&pos, &style) in &result {
        if Some(style) != last_style {
            final_result.insert(pos, style);
            last_style = Some(style);
        }
    }

    final_result
}

pub async fn render_tree_sitter_extmarks(bufs: ResMut<Buffers>, highlights: Res<HighlightMap>) {
    get!(mut bufs, highlights);

    let mut buf = bufs.cur_buffer_mut().await;

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
