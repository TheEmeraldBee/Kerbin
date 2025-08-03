use std::{collections::BTreeMap, io::ErrorKind, path::Path, sync::Arc};

mod buffers;
pub use buffers::*;
use stategine::prelude::Res;

use crate::*;

use ascii_forge::prelude::*;
use tree_sitter::{InputEdit, Parser, Point, Query, Tree};

pub mod action;
use action::*;

fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

#[derive(Default)]
pub struct ChangeGroup(Vec<Box<dyn BufferAction>>);

#[derive(rune::Any)]
pub struct TextBuffer {
    pub lines: Vec<String>,
    pub path: String,
    pub cursor_pos: Vec2,
    undo_stack: Vec<ChangeGroup>,
    redo_stack: Vec<ChangeGroup>,
    current_change: Option<ChangeGroup>,

    pub scroll: usize,
    pub h_scroll: usize,

    parser: Option<Parser>,
    tree: Option<Tree>,

    query: Option<Arc<Query>>,

    highlights: BTreeMap<usize, ContentStyle>,

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
            h_scroll: 0,

            parser: None,
            tree: None,
            query: None,
            highlights: BTreeMap::new(),

            tree_sitter_dirty: false,
            tree_sitter_full_clean: false,
            changes: vec![],
        }
    }

    pub fn open(path: String, grammar_manager: &mut GrammarManager, theme: &Theme) -> Self {
        let path_str = path.to_string();
        let mut parser = None;
        let mut tree = None;
        let mut query = None;
        let mut highlights = BTreeMap::new();

        if let Some(ext) = Path::new(&path_str).extension().and_then(|s| s.to_str()) {
            if let Some((language, q)) = grammar_manager.get_language_and_query_for_ext(ext) {
                let mut p = Parser::new();
                p.set_language(&language).unwrap();
                parser = Some(p);
                query = Some(q.clone());
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
                highlights = highlight(&lines, t, q, theme);
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
            h_scroll: 0,

            parser,
            tree,
            query,
            highlights,

            tree_sitter_dirty: false,
            tree_sitter_full_clean: false,
            changes: vec![],
        }
    }

    pub fn action(&mut self, action: impl BufferAction) -> bool {
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
        success
    }

    pub fn insert_char_at_cursor(&mut self, chr: char) -> bool {
        self.action(Insert {
            pos: self.cursor_pos,
            content: chr.to_string(),
        })
    }

    pub fn remove_chars_relative(&mut self, offset: i16, mut count: usize) -> bool {
        let mut pos = self.cursor_pos;
        let mut new_pos = pos.x as i16 + offset;
        if new_pos < 0 {
            count = count.saturating_add_signed(new_pos as isize);
            new_pos = 0;
        }
        pos.x = new_pos as u16;
        if count == 0 {
            return true;
        }
        self.action(Delete { pos, len: count })
    }

    pub fn insert_newline_relative(&mut self, offset: i16) -> bool {
        let mut pos = self.cursor_pos;
        pos.x = pos.x.saturating_add_signed(offset);
        self.action(InsertNewline { pos })
    }

    pub fn create_line(&mut self, offset: i16) -> bool {
        let line_idx = (self.cursor_pos.y as i16 + offset) as usize;
        self.action(InsertLine {
            line_idx,
            content: String::new(),
        })
    }

    pub fn delete_line(&mut self, offset: i16) -> bool {
        let line_idx = (self.cursor_pos.y as i16 + offset) as usize;
        self.action(DeleteLine { line_idx })
    }

    pub fn join_line_relative(&mut self, offset: i16) -> bool {
        let line_idx = (self.cursor_pos.y as i16 + offset) as usize;
        self.action(JoinLine { line_idx })
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

    pub fn refresh_highlights(&mut self, theme: &Theme) {
        self.tree_sitter_full_clean = true;
        self.update_tree_and_highlights(theme);
    }

    fn update_tree_and_highlights(&mut self, theme: &Theme) {
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
                self.highlights = highlight(&self.lines, t, q, theme);
            }

            self.tree_sitter_dirty = false;
            self.tree_sitter_full_clean = false;
            self.changes.clear();
        }
    }

    pub fn scroll_lines(&mut self, delta: isize) -> bool {
        if delta == 0 {
            return true;
        }
        let old_scroll = self.scroll;
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .clamp(0, self.lines.len().saturating_sub(1));
        self.scroll != old_scroll
    }

    pub fn scroll_horizontal(&mut self, delta: isize) -> bool {
        if delta == 0 {
            return true;
        }
        let old_scroll = self.h_scroll;
        self.h_scroll = self.h_scroll.saturating_add_signed(delta);
        self.h_scroll != old_scroll
    }

    pub fn write_file(&mut self, path: Option<String>) {
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

    pub fn set_cur_line(&mut self, line: String) {
        self.lines
            .get_mut(self.cursor_pos.y as usize)
            .map(|x| *x = line);
    }

    pub fn cur_line_mut(&mut self) -> Option<&mut String> {
        self.lines.get_mut(self.cursor_pos.y as usize)
    }

    pub fn move_cursor(&mut self, x: i16, y: i16) -> bool {
        if x == 0 && y == 0 {
            return true;
        }
        let old_pos = self.cursor_pos;

        self.cursor_pos.x = self.cursor_pos.x.saturating_add_signed(x);
        self.cursor_pos.y = self
            .cursor_pos
            .y
            .saturating_add_signed(y)
            .clamp(0, (self.lines.len() as u16).saturating_sub(1));

        let line_length = self.cur_line().chars().count();

        self.cursor_pos.x = self.cursor_pos.x.clamp(0, line_length as u16);
        self.cursor_pos != old_pos
    }

    fn render(&self, mut loc: Vec2, buffer: &mut Buffer, theme: &Theme) -> Vec2 {
        let default_style = ContentStyle::new().with(Color::Rgb {
            r: 205,
            g: 214,
            b: 244,
        });

        let line_style = theme
            .get("ui.linenum")
            .map(|x| x.to_content_style())
            .unwrap_or(ContentStyle::new().dark_grey());

        let mut byte_offset: usize = self
            .lines
            .iter()
            .take(self.scroll)
            .map(|l| l.len() + 1)
            .sum();

        let gutter_width = 6;

        for (i, line) in self
            .lines
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(buffer.size().y as usize)
        {
            let mut num_line = i.to_string();
            if num_line.len() > 5 {
                num_line = num_line[0..5].to_string();
            }
            if i == self.cursor_pos.y as usize {
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

            render!(buffer, loc => [StyledContent::new(line_style, num_line.to_string())]);

            let mut current_style = self
                .highlights
                .range(..=byte_offset)
                .next_back()
                .map(|(_, style)| *style)
                .unwrap_or(default_style);

            for (current_char_col, (char_byte_idx, ch)) in (0usize..).zip(line.char_indices()) {
                let absolute_char_byte_idx = byte_offset + char_byte_idx;

                if let Some(new_style) = self.highlights.get(&absolute_char_byte_idx) {
                    if new_style.foreground_color.is_none() {
                        current_style = self
                            .highlights
                            .range(..=absolute_char_byte_idx)
                            .next_back()
                            .map(|(_, s)| *s)
                            .unwrap_or(default_style);
                    } else {
                        current_style = *new_style;
                    }
                }

                if current_char_col >= self.h_scroll {
                    let render_col = (current_char_col - self.h_scroll) as u16 + gutter_width;

                    if render_col >= buffer.size().x {
                        break;
                    }

                    render!(buffer, loc + vec2(render_col, 0) => [StyledContent::new(current_style, ch)]);
                }
            }

            loc.y += 1;
            byte_offset += line.len() + 1;
        }

        loc
    }
}

/// A system that processes any pending changes in the active buffer
/// to update its syntax highlighting.
pub fn update_highlights(buffers: Res<Buffers>, theme: Res<Theme>) {
    buffers
        .cur_buffer()
        .borrow_mut()
        .update_tree_and_highlights(&theme);
}
