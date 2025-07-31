use std::{io::ErrorKind, ops::Range, path::Path, sync::Arc};

mod buffers;
pub use buffers::*;
use stategine::prelude::{Res, ResMut};

use crate::*;

use ascii_forge::prelude::*;
use tree_sitter::{InputEdit, Parser, Point, Query, Tree};

pub mod action;
use action::*;

/// Converts a character-based column index to a byte-based one for a given string.
fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

#[derive(Default)]
pub struct ChangeGroup(Vec<Box<dyn BufferAction>>);

pub struct TextBuffer {
    pub lines: Vec<String>,
    pub path: String,
    pub cursor_pos: Vec2,
    undo_stack: Vec<ChangeGroup>,
    redo_stack: Vec<ChangeGroup>,
    current_change: Option<ChangeGroup>,

    pub scroll: usize,

    // Tree-sitter fields
    parser: Option<Parser>,
    tree: Option<Tree>,

    query: Option<Arc<Query>>,

    highlights: Vec<(Range<usize>, ContentStyle)>,

    tree_sitter_dirty: bool,
    tree_sitter_full_clean: bool,
    changes: Vec<InputEdit>,
}

impl TextBuffer {
    pub fn scratch() -> Self {
        Self {
            lines: vec!["".to_string()],
            path: "<scratch>".to_string(),
            cursor_pos: vec2(0, 0),
            undo_stack: vec![],
            redo_stack: vec![],
            current_change: None,

            scroll: 0,

            parser: None,
            tree: None,
            query: None,
            highlights: vec![],

            tree_sitter_dirty: false,
            tree_sitter_full_clean: false,
            changes: vec![],
        }
    }

    pub fn open(
        path: impl ToString,
        grammar_manager: &mut GrammarManager,
        hl_config: &HighlightConfiguration,
    ) -> Self {
        let path_str = path.to_string();
        let mut parser = None;
        let mut tree = None;
        let mut query = None;
        let mut highlights = Vec::new();

        if let Some(ext) = Path::new(&path_str).extension().and_then(|s| s.to_str()) {
            if let Some((language, q)) = grammar_manager.get_language_and_query_for_ext(ext) {
                let mut p = Parser::new();
                p.set_language(&language).unwrap();
                parser = Some(p);
                query = Some(q.clone()); // Clone the query to store it.
            }
        }

        let lines = match std::fs::read_to_string(&path_str) {
            Ok(t) => t.lines().map(|x| x.to_string()).collect::<Vec<String>>(),
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::error!("{e} when opening file, {path_str}");
                }
                vec!["".to_string()]
            }
        };

        if let (Some(p), Some(q)) = (parser.as_mut(), query.as_ref()) {
            let text = lines.join("\n");
            tree = p.parse(&text, None);
            if let Some(t) = &tree {
                highlights = highlight(&lines, t, q, hl_config);
            }
        }

        Self {
            lines,
            path: path_str,
            cursor_pos: vec2(0, 0),
            undo_stack: vec![],
            redo_stack: vec![],
            current_change: None,

            scroll: 0,

            parser,
            tree,
            query,
            highlights,

            tree_sitter_dirty: false,
            tree_sitter_full_clean: false,
            changes: vec![],
        }
    }

    /// Executes an action, records its inverse for undo, and clears the redo stack.
    pub fn action(&mut self, action: impl BufferAction) {
        if self.current_change.is_none() {
            self.start_change_group();
        }

        let (success, inverse) = action.apply(self);

        if success {
            if let Some(group) = self.current_change.as_mut() {
                group.0.push(inverse);
            }
            self.redo_stack.clear();
        }
    }

    pub fn insert_char_at_cursor(&mut self, chr: char) {
        self.action(Insert {
            pos: self.cursor_pos,
            content: chr.to_string(),
        });
    }

    pub fn remove_chars_relative(&mut self, offset: i16, mut count: usize) {
        let mut pos = self.cursor_pos;
        let mut new_pos = pos.x as i16 + offset;
        if new_pos < 0 {
            count = count.saturating_add_signed(new_pos as isize);
            new_pos = 0;
        }
        pos.x = new_pos as u16;
        if count == 0 {
            // We've checked positions, and we aren't really deleting anything
            return;
        }
        self.action(Delete { pos, len: count });
    }

    pub fn insert_newline_relative(&mut self, offset: i16) {
        let mut pos = self.cursor_pos;
        pos.x = pos.x.saturating_add_signed(offset);
        self.action(InsertNewline { pos });
    }

    pub fn create_line(&mut self, offset: i16) {
        let line_idx = (self.cursor_pos.y as i16 + offset) as usize;
        self.action(InsertLine {
            line_idx,
            content: String::new(),
        });
    }

    pub fn delete_line(&mut self, offset: i16) {
        let line_idx = (self.cursor_pos.y as i16 + offset) as usize;
        self.action(DeleteLine { line_idx });
    }

    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup(Vec::new()));
    }

    pub fn commit_change_group(&mut self) {
        if let Some(group) = self.current_change.take() {
            if !group.0.is_empty() {
                self.undo_stack.push(group);
            }
        }
    }

    pub fn undo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.undo_stack.pop() {
            let mut redo_group = Vec::new();
            for action in group.0.iter().rev() {
                let (_, inverse) = action.apply(self);
                redo_group.push(inverse);
            }
            redo_group.reverse();
            self.redo_stack.push(ChangeGroup(redo_group));
        }
    }

    pub fn redo(&mut self) {
        if let Some(group) = self.redo_stack.pop() {
            let mut undo_group = Vec::new();
            for action in group.0.iter().rev() {
                let (_, inverse) = action.apply(self);
                undo_group.push(inverse);
            }
            undo_group.reverse();
            self.undo_stack.push(ChangeGroup(undo_group));
        }
    }

    fn get_byte_offset(&self, point: Point) -> usize {
        self.lines[..point.row]
            .iter()
            .map(|line| line.len() + 1)
            .sum::<usize>()
            + point.column
    }

    fn update_tree_and_highlights(&mut self, hl_config: &HighlightConfiguration) {
        if !self.tree_sitter_dirty && !self.tree_sitter_full_clean {
            return;
        }

        if let Some(parser) = self.parser.as_mut() {
            let text = self.lines.join("\n");
            let Some(t) = self.tree.as_mut() else {
                return;
            };

            if self.tree_sitter_full_clean {
                self.tree = parser.parse(&text, None);
            } else {
                for edit in &self.changes {
                    t.edit(edit);
                }
                self.tree = parser.parse(&text, self.tree.as_ref());
            }
            if let (Some(t), Some(q)) = (self.tree.as_ref(), self.query.as_ref()) {
                self.highlights = highlight(&self.lines, t, q, hl_config);
            }

            self.tree_sitter_dirty = false;
            self.tree_sitter_full_clean = false;
            self.changes.clear();
        }
    }

    pub fn scroll_lines(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .clamp(0, self.lines.len().saturating_sub(1));
    }

    pub fn write_file(&mut self, path: Option<impl ToString>) {
        if self.path == "<scratch>" {
            tracing::warn!("unable to save scratch files!");
            return;
        }

        if let Some(new_path) = path {
            self.path = new_path.to_string();
        }

        std::fs::write(&self.path, self.lines.join("\n")).unwrap();
    }

    pub fn cur_line(&self) -> String {
        self.lines
            .get(self.cursor_pos.y as usize)
            .cloned()
            .unwrap_or_default()
    }

    pub fn cur_line_mut(&mut self) -> Option<&mut String> {
        self.lines.get_mut(self.cursor_pos.y as usize)
    }

    pub fn move_cursor(&mut self, x: i16, y: i16) {
        self.cursor_pos.x = self.cursor_pos.x.saturating_add_signed(x);
        self.cursor_pos.y = self
            .cursor_pos
            .y
            .saturating_add_signed(y)
            .clamp(0, (self.lines.len() as u16).saturating_sub(1));

        let line_length = self.cur_line().len();

        self.cursor_pos.x = self.cursor_pos.x.clamp(0, line_length as u16);
    }
}

impl Render for TextBuffer {
    fn render(&self, mut loc: Vec2, buffer: &mut Buffer) -> Vec2 {
        let mut byte_offset: usize = self
            .lines
            .iter()
            .take(self.scroll)
            .map(|l| l.len() + 1)
            .sum();

        for line in self.lines.iter().skip(self.scroll) {
            let current_style = ContentStyle::new().with(Color::Rgb {
                r: 205,
                g: 214,
                b: 244,
            });
            let mut last_byte = 0;

            for (range, style) in &self.highlights {
                if range.start > byte_offset + line.len() || range.end <= byte_offset {
                    continue;
                }

                let start = range.start.saturating_sub(byte_offset);
                let end = range.end.saturating_sub(byte_offset);

                if start > last_byte {
                    render!(buffer, loc + vec2(last_byte as u16, 0) => [StyledContent::new(current_style, &line[last_byte..start])]);
                }
                render!(buffer, loc + vec2(start as u16, 0) => [StyledContent::new(*style, &line[start..end.min(line.len())])]);
                last_byte = end.min(line.len());
            }

            if last_byte < line.len() {
                render!(buffer, loc + vec2(last_byte as u16, 0) => [StyledContent::new(current_style, &line[last_byte..])]);
            }

            loc.y += 1;
            byte_offset += line.len() + 1;
        }

        loc
    }
}

/// A system that processes any pending changes in the active buffer
/// to update its syntax highlighting.
pub fn update_highlights(mut buffers: ResMut<Buffers>, hl_config: Res<HighlightConfiguration>) {
    buffers
        .cur_buffer_mut()
        .update_tree_and_highlights(&hl_config);
}
