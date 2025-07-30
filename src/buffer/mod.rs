use std::{io::ErrorKind, ops::Range, path::Path, sync::Arc};

mod buffers;
pub use buffers::*;
use stategine::prelude::{Res, ResMut};

use crate::*;

use ascii_forge::prelude::*;
use tree_sitter::{InputEdit, Parser, Point, Query, Tree};

/// Converts a character-based column index to a byte-based one for a given string.
fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

#[derive(Clone)]
pub enum UndoAction {
    // To undo an insertion, we delete characters at a position.
    DeleteChars(Vec2, usize),
    // To undo a deletion, we insert characters at a position.
    InsertChars(Vec2, String),
    // To undo a line insertion, we delete the line.
    DeleteLine(usize),
    // To undo a line deletion, we re-insert it with its content.
    InsertLine(usize, String),
}

#[derive(Default, Clone)]
pub struct ChangeGroup(Vec<UndoAction>);

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

    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup::default());
        self.redo_stack.clear();
    }

    pub fn commit_change_group(&mut self) {
        if let Some(change) = self.current_change.take() {
            if !change.0.is_empty() {
                self.undo_stack.push(change);
            }
        }
    }

    pub fn undo(&mut self) {
        // First, commit any pending changes from insert mode, etc.
        self.commit_change_group();

        if let Some(group) = self.undo_stack.pop() {
            let mut redo_group = ChangeGroup::default();
            // Apply the undo actions in reverse to correctly restore state.
            for action in group.0.iter().rev() {
                // apply_undo_action modifies the text and returns the inverse action.
                let inverse_action = self.apply_undo_action(action);
                redo_group.0.push(inverse_action);
            }
            // The actions to redo are the inverses, but in forward order.
            self.redo_stack.push(redo_group);

            // The tree is now in an arbitrary state. A full re-parse is required.
            // We signal this by clearing incremental changes and setting the dirty flag.
            self.changes.clear();
            self.tree_sitter_full_clean = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(group) = self.redo_stack.pop() {
            let mut undo_group = ChangeGroup::default();
            // Apply the redo actions in reverse.
            for action in group.0.iter().rev() {
                let inverse_action = self.apply_undo_action(action);
                undo_group.0.push(inverse_action);
            }
            // The actions to undo the redo are the inverses, in forward order.
            self.undo_stack.push(undo_group);

            // The tree is dirty, signal a full re-parse.
            self.changes.clear();
            self.tree_sitter_full_clean = true;
        }
    }

    // Applies an undo action and returns its inverse.
    fn apply_undo_action(&mut self, action: &UndoAction) -> UndoAction {
        match action {
            UndoAction::DeleteChars(pos, count) => {
                let line = &mut self.lines[pos.y as usize];
                let start = pos.x as usize;
                let end = start + count;
                let removed: String = line.drain(start..end).collect();
                self.cursor_pos = *pos;
                UndoAction::InsertChars(*pos, removed)
            }
            UndoAction::InsertChars(pos, text) => {
                self.lines[pos.y as usize].insert_str(pos.x as usize, text);
                self.cursor_pos = *pos;
                UndoAction::DeleteChars(*pos, text.len())
            }
            UndoAction::DeleteLine(y) => {
                let content = self.lines.remove(*y);
                self.move_cursor(0, 0);
                UndoAction::InsertLine(*y, content)
            }
            UndoAction::InsertLine(y, content) => {
                self.lines.insert(*y, content.clone());
                self.move_cursor(0, 0);
                UndoAction::DeleteLine(*y)
            }
        }
    }

    fn record_action(&mut self, action: UndoAction) {
        if let Some(change) = &mut self.current_change {
            change.0.push(action);
        }
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

    pub fn insert_char_at_cursor(&mut self, chr: char) -> bool {
        let pos = self.cursor_pos;
        let line_idx = pos.y as usize;
        if let Some(line) = self.lines.get_mut(line_idx) {
            line.insert(pos.x as usize, chr);

            let start_byte_col = char_to_byte_index(line, pos.x as usize);
            let start_pos = Point::new(line_idx, start_byte_col);
            let start_byte_offset = self.get_byte_offset(start_pos);

            self.record_action(UndoAction::DeleteChars(pos, 1));

            let new_end_byte_col = start_byte_col + chr.len_utf8();
            let edit = InputEdit {
                start_byte: start_byte_offset,
                old_end_byte: start_byte_offset,
                new_end_byte: start_byte_offset + chr.len_utf8(),
                start_position: start_pos,
                old_end_position: start_pos,
                new_end_position: Point::new(line_idx, new_end_byte_col),
            };
            self.changes.push(edit);
            self.tree_sitter_dirty = true;
            true
        } else {
            false
        }
    }

    pub fn remove_chars_relative(&mut self, offset: i16, count: usize) -> bool {
        let line_idx = self.cursor_pos.y as usize;
        if let Some(line) = self.lines.get_mut(line_idx) {
            let start_char = (self.cursor_pos.x as i16 + offset) as usize;
            let end_char = start_char + count;

            let start_byte_col = char_to_byte_index(line, start_char);
            let end_byte_col = char_to_byte_index(line, end_char);

            let removed: String = line.drain(start_byte_col..end_byte_col).collect();

            if start_byte_col >= line.len() {
                return false;
            }

            let start_pos = Point::new(line_idx, start_byte_col);
            let start_byte_offset = self.get_byte_offset(start_pos);
            let old_end_pos = Point::new(line_idx, end_byte_col);
            let old_end_byte_offset = self.get_byte_offset(old_end_pos);

            self.record_action(UndoAction::InsertChars(self.cursor_pos, removed));

            let edit = InputEdit {
                start_byte: start_byte_offset,
                old_end_byte: old_end_byte_offset,
                new_end_byte: start_byte_offset,
                start_position: start_pos,
                old_end_position: old_end_pos,
                new_end_position: start_pos,
            };
            self.changes.push(edit);
            self.tree_sitter_dirty = true;
            true
        } else {
            false
        }
    }

    pub fn insert_newline_relative(&mut self, offset: i16) {
        let line_idx = self.cursor_pos.y as usize;
        let char_col = (self.cursor_pos.x as i16 + offset) as usize;
        let byte_col = char_to_byte_index(&self.lines[line_idx], char_col);

        let start_pos = Point::new(line_idx, byte_col);
        let start_byte_offset = self.get_byte_offset(start_pos);

        let (lhs, rhs) = self.lines[line_idx].split_at(byte_col);
        let (lhs, rhs) = (lhs.to_string(), rhs.to_string());
        *self.lines.get_mut(line_idx).unwrap() = lhs.to_string();
        self.lines.insert(line_idx + 1, rhs.to_string());
        self.record_action(UndoAction::DeleteLine(line_idx + 1));

        let edit = InputEdit {
            start_byte: start_byte_offset,
            old_end_byte: start_byte_offset,
            new_end_byte: start_byte_offset + 1, // for the newline
            start_position: start_pos,
            old_end_position: start_pos,
            new_end_position: Point::new(line_idx + 1, 0),
        };
        self.changes.push(edit);
        self.tree_sitter_dirty = true;
    }

    pub fn delete_line(&mut self, offset: i16) {
        let line_idx = (self.cursor_pos.y as i16 + offset) as usize;
        if line_idx >= self.lines.len() {
            return;
        }

        let start_pos = Point::new(line_idx, 0);
        let start_byte_offset = self.get_byte_offset(start_pos);
        let removed_len = self.lines[line_idx].len() + 1;

        let removed = self.lines.remove(line_idx);
        self.record_action(UndoAction::InsertLine(line_idx, removed));
        self.move_cursor(0, 0);

        let edit = InputEdit {
            start_byte: start_byte_offset,
            old_end_byte: start_byte_offset + removed_len,
            new_end_byte: start_byte_offset,
            start_position: start_pos,
            old_end_position: Point::new(line_idx + 1, 0),
            new_end_position: start_pos,
        };
        self.changes.push(edit);
        self.tree_sitter_dirty = true;
    }

    pub fn create_line(&mut self, offset: i16) {
        let line_idx = (self.cursor_pos.y as i16).saturating_add(offset) as usize;

        self.lines.insert(line_idx, String::default());

        self.tree_sitter_full_clean = true;
        self.record_action(UndoAction::DeleteLine(line_idx));
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
