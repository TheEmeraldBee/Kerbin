pub mod action;
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

pub use action::*;

pub mod buffers;
pub use buffers::*;

use ascii_forge::prelude::*;

pub(crate) fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

#[derive(Default)]
pub struct ChangeGroup(Vec<Box<dyn BufferAction>>);

pub struct TextBuffer {
    lines: Vec<String>,

    pub path: String,
    pub ext: String,

    pub col: usize,
    pub row: usize,

    current_change: Option<ChangeGroup>,

    undo_stack: Vec<ChangeGroup>,
    redo_stack: Vec<ChangeGroup>,

    pub scroll: usize,
    pub h_scroll: usize,
}

impl TextBuffer {
    pub fn scratch() -> Self {
        Self {
            lines: Vec::from_iter(["".into()].into_iter()),

            path: "<scratch>".into(),
            ext: "".into(),

            col: 0,
            row: 0,

            current_change: None,

            undo_stack: vec![],
            redo_stack: vec![],

            scroll: 0,
            h_scroll: 0,
        }
    }

    pub fn open(path_str: String) -> Self {
        let mut found_ext = "".to_string();

        let path = Path::new(&path_str)
            .canonicalize()
            .unwrap_or(PathBuf::from(&path_str));

        let lines = match std::fs::read_to_string(&path_str) {
            Ok(t) => t.lines().map(|x| x.to_string()).collect::<Vec<String>>(),
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    // TODO: tracing::error!("{e} when opening file, {path_str}");
                }
                vec!["".to_string()]
            }
        };

        Self {
            lines,
            path: path.to_str().map(|x| x.to_string()).unwrap_or_default(),
            ext: found_ext,

            row: 0,
            col: 0,

            undo_stack: vec![],
            redo_stack: vec![],
            current_change: None,

            scroll: 0,
            h_scroll: 0,
        }
    }

    pub fn action(&mut self, action: impl BufferAction) -> bool {
        if self.current_change.is_none() {
            self.start_change_group();
        }

        let res = action.apply(self);

        if res.success {
            if let Some(group) = self.current_change.as_mut() {
                group.0.push(res.action)
            }

            self.redo_stack.clear();
        }

        res.success
    }

    pub fn undo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.undo_stack.pop() {
            let mut redo_group = vec![];

            for action in group.0.into_iter() {
                let ActionResult { action, .. } = action.apply(self);
                redo_group.push(action);
            }

            self.redo_stack.push(ChangeGroup(redo_group));
        }
    }

    pub fn redo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.redo_stack.pop() {
            let mut undo_group = vec![];

            for action in group.0.into_iter() {
                let ActionResult { action, .. } = action.apply(self);
                undo_group.push(action);
            }

            self.undo_stack.push(ChangeGroup(undo_group));
        }
    }

    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup(vec![]));
    }

    pub fn commit_change_group(&mut self) {
        if let Some(group) = self.current_change.take() {
            if !group.0.is_empty() {
                self.undo_stack.push(group)
            }
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
            //TODO: Print to log
            return;
        }

        if let Some(new_path) = path {
            self.path = Path::new(&new_path)
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
        }
    }

    pub fn cur_line(&self) -> String {
        self.lines.get(self.row).cloned().unwrap_or_default()
    }

    pub fn cur_line_mut(&mut self) -> Option<&mut String> {
        self.lines.get_mut(self.row)
    }

    pub fn move_cursor(&mut self, rows: isize, cols: isize) -> bool {
        if rows == 0 && cols == 0 {
            return true;
        }

        let old_col = self.col;
        let old_row = self.row;

        self.col = self.col.saturating_add_signed(cols);
        self.row = self
            .row
            .saturating_add_signed(rows)
            .clamp(0, self.lines.len().saturating_sub(1));

        let line_length = self.cur_line().chars().count();

        self.col = self.col.clamp(0, line_length);
        self.row != old_row || self.col != old_col
    }

    pub fn render(&self, mut loc: Vec2, buffer: &mut Buffer) {
        let mut byte_offset: usize = self
            .lines
            .iter()
            .take(self.scroll)
            .map(|l| l.len() + 1)
            .sum();

        let gutter_width = 6;
        let start_x = loc.x;
        for (i, line) in self
            .lines
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(buffer.size().y as usize)
        {
            loc.x = start_x;
            let mut num_line = i.to_string();

            if num_line.len() > 5 {
                num_line = num_line[0..5].to_string();
            }

            if i == self.row as usize {
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

            render!(buffer, loc => [num_line]);
            loc.x += gutter_width;

            render!(buffer, loc => [line]);

            loc.y += 1;
            byte_offset += line.len() + 1;
        }
    }
}
