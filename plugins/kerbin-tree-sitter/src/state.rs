use kerbin_core::{
    ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle},
    *,
};

use std::{
    collections::{BTreeMap, VecDeque},
    ops::Range,
    sync::Arc,
};

use ropey::Rope;
use tree_sitter::{InputEdit, Parser, Query, QueryCursor, StreamingIterator, TextProvider, Tree};

use crate::{GrammarManager, TreeSitterStates};

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

        if let Some(tree) = &mut self.tree {
            for edit in &self.changes {
                tree.edit(edit);
            }
        }

        self.tree = self.parser.parse_with_options(
            &mut |byte, _point| {
                let res = text.get_chunk(byte);
                res.map(|x| &x.0[(byte - x.1)..]).unwrap_or_default()
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

pub async fn render_tree_sitter_buffer(
    chunk: Chunk<BufferChunk>,
    theme: Res<Theme>,
    modes: Res<ModeStack>,
    bufs: Res<Buffers>,
    ts_states: Res<TreeSitterStates>,
) {
    let mut chunk = chunk.get().unwrap();
    get!(bufs, modes, theme, ts_states);
    let mut loc = vec2(0, 0);

    let buf = bufs.cur_buffer();
    let buf = buf.read().unwrap();

    let state = ts_states.bufs.get(&buf.path).unwrap();

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

    // Buffer should always be 0 priority (should always be set)
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

    let mut cursor_style = None;

    while !cursor_parts.is_empty() {
        if let Some(s) = theme.get(&format!(
            "ui.cursor.{}",
            cursor_parts
                .iter()
                .cloned()
                .reduce(|l, r| format!("{l}.{r}"))
                .unwrap()
        )) {
            cursor_style = Some(s);
            break;
        }
        cursor_parts.pop_front();
    }

    let cursor_style = match cursor_style {
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

        if let Some(ts) = &state {
            let mut current_style = ts
                .highlights
                .range(..=byte_offset)
                .next_back()
                .map(|(_, style)| *style)
                .unwrap_or(default_style);

            for (char_col, (char_byte_idx, ch)) in line.char_indices().enumerate() {
                let absolute_byte_idx = byte_offset + char_byte_idx;

                if let Some(new_style) = ts.highlights.get(&absolute_byte_idx) {
                    current_style = *new_style;
                }

                let mut in_selection = false;
                let mut is_cursor = false;
                for (i, cursor) in buf.cursors.iter().enumerate() {
                    if cursor.get_cursor_byte() == absolute_byte_idx {
                        // Don't style the primary cursor, only non-primary ones.
                        if i != buf.primary_cursor {
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
            // Fallback for files with no syntax highlighting
            render!(chunk, loc => [ line.to_string() ]);
        }

        loc.y += 1;
        byte_offset += line.len();
        i += 1;
    }
}
