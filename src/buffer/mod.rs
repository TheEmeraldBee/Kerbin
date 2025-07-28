use std::io::ErrorKind;

mod buffers;
pub use buffers::*;

use ascii_forge::prelude::*;

// Represents the inverse of an action to be stored in a ChangeGroup
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

// A group of actions that should be undone/redone together.
#[derive(Default, Clone)]
pub struct ChangeGroup(Vec<UndoAction>);

pub struct TextBuffer {
    pub lines: Vec<String>,
    pub path: String,
    pub cursor_pos: Vec2,
    // A stack of changes that can be undone.
    undo_stack: Vec<ChangeGroup>,
    // A stack of changes that can be redone.
    redo_stack: Vec<ChangeGroup>,
    // The current group of changes being recorded.
    current_change: Option<ChangeGroup>,
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
        }
    }

    pub fn open(path: impl ToString) -> Self {
        let path = path.to_string();

        let lines = match std::fs::read_to_string(&path) {
            Ok(t) => t.lines().map(|x| x.to_string()).collect::<Vec<String>>(),
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::error!("{e} when opening file, {path}");
                }
                vec!["".to_string()]
            }
        };

        Self {
            lines,
            path,
            cursor_pos: vec2(0, 0),
            undo_stack: vec![],
            redo_stack: vec![],
            current_change: None,
        }
    }

    pub fn start_change_group(&mut self) {
        // When a new change is initiated, commit any pending changes.
        self.commit_change_group();
        // Start a new group.
        self.current_change = Some(ChangeGroup::default());
        // A new change invalidates the redo history.
        self.redo_stack.clear();
    }

    pub fn commit_change_group(&mut self) {
        if let Some(change) = self.current_change.take() {
            // Only add non-empty change groups to the stack.
            if !change.0.is_empty() {
                self.undo_stack.push(change);
            }
        }
    }

    pub fn undo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.undo_stack.pop() {
            // Apply the inverse actions in reverse order.
            for action in group.0.iter().rev() {
                self.apply_undo_action(action);
            }
            self.redo_stack.push(group);
        }
    }

    pub fn redo(&mut self) {
        if let Some(group) = self.redo_stack.pop() {
            // To redo, we need to reverse the undo actions.
            let mut inverted_group = ChangeGroup::default();
            // Apply the actions in forward order.
            for action in group.0.iter() {
                let inverse = self.apply_undo_action(action);
                inverted_group.0.push(inverse);
            }
            self.undo_stack.push(inverted_group);
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

    pub fn write_file(&self, path: Option<impl ToString>) {
        let path = match path {
            Some(p) => p.to_string(),
            None => self.path.clone(),
        };

        std::fs::write(path, self.lines.join("\n")).unwrap();
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
        if let Some(line) = self.cur_line_mut() {
            line.insert(pos.x as usize, chr);
            self.record_action(UndoAction::DeleteChars(pos, 1));
            true
        } else {
            false
        }
    }

    pub fn remove_chars_relative(&mut self, offset: i16, count: usize) -> bool {
        let mut pos = self.cursor_pos;
        pos.x = pos.x.saturating_add_signed(offset);

        if let Some(line) = self.cur_line_mut() {
            let start = pos.x as usize;
            if start >= line.len() {
                return false;
            }
            let end = (start + count).min(line.len());
            let removed: String = line.drain(start..end).collect();
            self.record_action(UndoAction::InsertChars(pos, removed));
            true
        } else {
            false
        }
    }

    pub fn insert_newline_relative(&mut self, offset: i16) {
        let cursor_x = self.cursor_pos.x as i16 + offset;
        let cursor_x = cursor_x.clamp(0, self.cur_line().len() as i16) as u16;

        let line = self.cur_line();
        let (lhs, rhs) = line.split_at(cursor_x as usize);

        *self.cur_line_mut().unwrap() = lhs.to_string();

        let line_idx = self.cursor_pos.y.saturating_add(1) as usize;
        self.lines.insert(line_idx, rhs.to_string());
        self.record_action(UndoAction::DeleteLine(line_idx));
    }

    pub fn create_line(&mut self, offset: i16) {
        let line_idx = (self.cursor_pos.y as i16).saturating_add(offset) as usize;
        self.lines.insert(line_idx, String::default());
        self.record_action(UndoAction::DeleteLine(line_idx));
    }

    pub fn delete_line(&mut self, offset: i16) {
        let line_idx = (self.cursor_pos.y as i16).saturating_add(offset) as usize;
        if line_idx >= self.lines.len() {
            return;
        }
        let removed = self.lines.remove(line_idx);
        self.record_action(UndoAction::InsertLine(line_idx, removed));
        self.move_cursor(0, 0);
    }
}

impl Render for TextBuffer {
    fn render(&self, mut loc: Vec2, buffer: &mut Buffer) -> Vec2 {
        let lines = &self.lines;
        let mut cursor_pos = self.cursor_pos;

        loc.y += 1;
        for (_i, line) in lines.iter().enumerate() {
            render!(buffer, loc => [
                line.as_str().with(Color::Rgb {
                    r: 205,
                    g: 214,
                    b: 244,
                })
            ]);
            loc.y += 1;
        }

        cursor_pos.y += 1;

        let style = buffer.get_mut(cursor_pos).style_mut();
        *style = style
            .on(Color::Rgb {
                r: 245,
                g: 224,
                b: 220,
            })
            .black();

        loc
    }
}
