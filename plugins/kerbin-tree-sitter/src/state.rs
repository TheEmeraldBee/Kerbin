use kerbin_core::{
    ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle},
    *,
};
use kerbin_macros::State;

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
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

pub async fn render_tree_sitter_buffer(
    chunk: Chunk<BufferChunk>,
    theme: Res<Theme>,
    modes: Res<ModeStack>,
    bufs: Res<Buffers>,
    highlights: Res<HighlightMap>,
) {
    let mut chunk = chunk.get().unwrap();
    get!(bufs, modes, theme, highlights);
    let mut loc = vec2(0, 0);

    let buf = bufs.cur_buffer();
    let buf = buf.read().unwrap();

    let highlight_map = highlights.0.get(&buf.path);

    let mut byte_offset = buf.rope.line_to_byte_idx(buf.scroll, LineType::LF_CR);

    let row = buf
        .rope
        .byte_to_line_idx(buf.primary_cursor().get_cursor_byte(), LineType::LF_CR);

    let cursor_byte = buf.primary_cursor().get_cursor_byte();
    let rope = &buf.rope;

    let current_row_idx = rope.byte_to_line_idx(cursor_byte, LineType::LF_CR);
    let line_start_byte_idx = rope.line_to_byte_idx(current_row_idx, LineType::LF_CR);
    let current_col_idx = rope
        .byte_to_char_idx(cursor_byte)
        .saturating_sub(rope.byte_to_char_idx(line_start_byte_idx));

    let cursor_style = match modes.get_mode() {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

    chunk.set_cursor(
        0,
        (
            current_col_idx as u16 + 6 - buf.h_scroll as u16,
            current_row_idx as u16 - buf.scroll as u16,
        )
            .into(),
        cursor_style,
    );

    let default_style = theme
        .get("ui.text")
        .unwrap_or_else(|| ContentStyle::new().with(Color::Rgb { r: 0, g: 0, b: 0 }));

    let line_style = theme
        .get("ui.linenum")
        .unwrap_or(ContentStyle::new().dark_grey());

    let sel_style = theme.get("ui.selection");

    let mut cursor_parts = modes
        .0
        .iter()
        .map(|x| x.to_string())
        .collect::<VecDeque<_>>();

    let mut cursor_style_theme = None;

    while !cursor_parts.is_empty() {
        if let Some(s) = theme.get(&format!(
            "ui.cursor.{}",
            cursor_parts
                .iter()
                .cloned()
                .reduce(|l, r| format!("{l}.{r}"))
                .unwrap()
        )) {
            cursor_style_theme = Some(s);
            break;
        }
        cursor_parts.pop_front();
    }

    let cursor_style = match cursor_style_theme {
        Some(s) => s,
        None => theme.get("ui.cursor").unwrap_or_default(),
    };

    let gutter_width = 6;
    let start_x = loc.x;

    let mut i = buf.scroll;

    for line in buf
        .rope
        .lines_at(buf.scroll, LineType::LF_CR)
        .take(chunk.size().y as usize)
    {
        loc.x = start_x;
        let mut num_line = (i + 1).to_string();
        if num_line.len() > 5 {
            num_line = num_line[0..5].to_string();
        }

        if i == row {
            num_line = format!(
                "{}{}",
                " ".repeat(4usize.saturating_sub(num_line.len())),
                num_line
            );
        } else {
            num_line = format!(
                "{}{}",
                " ".repeat(5usize.saturating_sub(num_line.len())),
                num_line
            );
        }

        render!(chunk, loc => [StyledContent::new(line_style, num_line)]);

        loc.x += gutter_width;

        if let Some(highlights_for_buf) = highlight_map {
            let mut current_style = highlights_for_buf
                .range(..=byte_offset)
                .next_back()
                .map(|(_, style)| *style)
                .unwrap_or(default_style);

            for (char_col, (char_byte_idx, ch)) in line.char_indices().enumerate() {
                let absolute_byte_idx = byte_offset + char_byte_idx;

                if let Some(new_style) = highlights_for_buf.get(&absolute_byte_idx) {
                    current_style = *new_style;
                }

                let mut in_selection = false;
                let mut is_cursor = false;
                for (cursor_idx, cursor) in buf.cursors.iter().enumerate() {
                    if cursor.get_cursor_byte() == absolute_byte_idx {
                        if cursor_idx != buf.primary_cursor {
                            is_cursor = true;
                        }
                        break;
                    }
                    if cursor.sel().contains(&absolute_byte_idx) {
                        in_selection = true;
                        break;
                    }
                }

                let resulting_style = match (is_cursor, in_selection) {
                    (false, false) => current_style,
                    (true, _) => cursor_style.combined_with(&cursor_style),
                    (false, true) => sel_style
                        .map(|x| x.combined_with(&current_style))
                        .unwrap_or(current_style.on_grey()),
                };

                if char_col >= buf.h_scroll {
                    let render_col = (char_col - buf.h_scroll) as u16;

                    if render_col >= chunk.size().x - 1 {
                        break;
                    }

                    render!(chunk, loc + vec2(render_col, 0) => [StyledContent::new(resulting_style, ch)]);
                }
            }
        } else {
            render!(chunk, loc => [ line.to_string() ]);
        }

        loc.y += 1;
        byte_offset += line.len();
        i += 1;
    }
}
