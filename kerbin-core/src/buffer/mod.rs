pub mod action;
use std::{
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

pub use action::*;

pub mod buffers;
pub use buffers::*;

pub mod tree_sitter;
use ::tree_sitter::{InputEdit, Point};
use tree_sitter::*;

use ascii_forge::prelude::*;

use crate::{GrammarManager, Theme};

pub(crate) fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

#[derive(Default)]
pub struct ChangeGroup(usize, usize, Vec<Box<dyn BufferAction>>);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum LineEnding {
    #[default]
    LF, // \n
    CRLF,  // \r\n
    CR,    // \r
    Mixed, // Different line endings found
    None,  // Empty file or no explicit line endings
}

pub struct TextBuffer {
    pub lines: Vec<String>,

    pub path: String,
    pub ext: String,

    pub col: usize,
    pub row: usize,

    pub selection: Option<(usize, std::ops::Range<usize>)>,

    current_change: Option<ChangeGroup>,

    undo_stack: Vec<ChangeGroup>,
    redo_stack: Vec<ChangeGroup>,

    pub ts_state: Option<TSState>,

    pub scroll: usize,
    pub h_scroll: usize,

    pub line_ending_style: LineEnding,
}

impl TextBuffer {
    pub fn scratch() -> Self {
        Self {
            lines: Vec::from_iter(["".into()]),

            path: "<scratch>".into(),
            ext: "".into(),

            col: 0,
            row: 0,

            selection: None,

            current_change: None,

            undo_stack: vec![],
            redo_stack: vec![],

            ts_state: None,

            scroll: 0,
            h_scroll: 0,

            line_ending_style: LineEnding::LF,
        }
    }

    pub fn open(path_str: String, grammar_manager: &mut GrammarManager, theme: &Theme) -> Self {
        let mut found_ext = "".to_string();

        let path = get_canonical_path_with_non_existent(&path_str);

        let text = match std::fs::read_to_string(&path_str) {
            Ok(t) => t,
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::error!("{e} when opening file, {path_str}");
                }
                "".to_string()
            }
        };

        let detected_line_ending = Self::detect_line_ending(&text);

        // Normalize all line endings to LF internally
        let normalized_text = text.replace("\r\n", "\n").replace('\r', "\n");

        let mut ts_state = None;

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            found_ext = ext.to_string();
            ts_state = TSState::init(ext, &normalized_text, grammar_manager, theme);
        }

        let lines = normalized_text
            .lines()
            .map(|x| x.to_string())
            .collect::<Vec<String>>();

        Self {
            lines,
            path: path.to_str().map(|x| x.to_string()).unwrap_or_default(),
            ext: found_ext,

            row: 0,
            col: 0,

            selection: None,

            undo_stack: vec![],
            redo_stack: vec![],
            current_change: None,

            ts_state,

            scroll: 0,
            h_scroll: 0,

            line_ending_style: detected_line_ending,
        }
    }

    fn detect_line_ending(text: &str) -> LineEnding {
        let mut has_lf = false;
        let mut has_crlf = false;
        let mut has_cr = false;

        for (i, c) in text.char_indices() {
            if c == '\n' {
                if i > 0 && text.chars().nth(i - 1) == Some('\r') {
                    has_crlf = true;
                } else {
                    has_lf = true;
                }
            } else if c == '\r' && text.chars().nth(i + 1) != Some('\n') {
                has_cr = true;
            }
        }

        match (has_lf, has_crlf, has_cr) {
            (true, false, false) => LineEnding::LF,
            (false, true, false) => LineEnding::CRLF,
            (false, false, true) => LineEnding::CR,
            (false, false, false) => LineEnding::None,
            _ => LineEnding::Mixed, // Any combination implies mixed
        }
    }

    // Helper to get the actual byte length of a line including its line ending
    fn get_full_line_byte_len(&self, row_idx: usize, style: LineEnding) -> usize {
        if row_idx >= self.lines.len() {
            return 0;
        }
        let line_content_byte_len = self.lines[row_idx].len();
        if row_idx < self.lines.len() - 1 || self.lines.len() == 1 {
            // All lines except the very last one have a line ending,
            // or if there's only one line, it effectively has a line ending in the logical model
            match style {
                LineEnding::LF => line_content_byte_len + 1,    // \n
                LineEnding::CRLF => line_content_byte_len + 2,  // \r\n
                LineEnding::CR => line_content_byte_len + 1,    // \r
                LineEnding::Mixed => line_content_byte_len + 1, // Default to \n for mixed or for newly inserted lines
                LineEnding::None => line_content_byte_len,      // No line ending if empty file
            }
        } else {
            // The last line does not have a trailing newline char in the file content
            line_content_byte_len
        }
    }

    pub fn get_edit_part(&self, row: usize, char_col: usize) -> (Point, usize) {
        let line = self.lines.get(row).map_or("", |l| l.as_str());
        let byte_col_in_line = char_to_byte_index(line, char_col);

        let mut cumulative_byte_offset = 0;
        for i in 0..row {
            cumulative_byte_offset += self.get_full_line_byte_len(i, self.line_ending_style);
        }
        cumulative_byte_offset += byte_col_in_line;

        (Point::new(row, byte_col_in_line), cumulative_byte_offset)
    }

    pub fn get_byte_offset_from_char_coords(&self, row: usize, col: usize) -> usize {
        let mut byte_offset = 0;
        for i in 0..row {
            byte_offset += self.lines[i].len() + 1;
        }
        byte_offset += self
            .lines
            .get(row)
            .and_then(|line| line.char_indices().nth(col))
            .map(|(idx, _)| idx)
            .unwrap_or_else(|| self.lines.get(row).map_or(0, |l| l.len()));
        byte_offset
    }

    pub fn register_input_edit(
        &mut self,
        start: (Point, usize),
        old_end: (Point, usize),
        new_end: (Point, usize),
    ) {
        if let Some(ts) = &mut self.ts_state {
            ts.tree_sitter_dirty = true;
            ts.changes.push(InputEdit {
                start_position: start.0,
                start_byte: start.1,

                old_end_position: old_end.0,
                old_end_byte: old_end.1,

                new_end_position: new_end.0,
                new_end_byte: new_end.1,
            })
        }
    }

    pub fn action(&mut self, action: impl BufferAction) -> bool {
        if self.current_change.is_none() {
            self.start_change_group();
        }

        let res = action.apply(self);

        if res.success {
            if let Some(group) = self.current_change.as_mut() {
                group.2.push(res.action)
            }

            self.redo_stack.clear();
        }

        res.success
    }

    pub fn undo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.undo_stack.pop() {
            let mut redo_group = vec![];

            let redo_row = self.row;
            let redo_col = self.col;

            for action in group.2.into_iter().rev() {
                let ActionResult { action, .. } = action.apply(self);
                redo_group.push(action);
            }

            self.row = group.0;
            self.col = group.1;

            redo_group.reverse();

            self.redo_stack
                .push(ChangeGroup(redo_row, redo_col, redo_group));
        }
    }

    pub fn redo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.redo_stack.pop() {
            let mut undo_group = vec![];

            let undo_row = self.row;
            let undo_col = self.col;

            for action in group.2.into_iter() {
                let ActionResult { action, .. } = action.apply(self);
                undo_group.push(action);
            }

            self.row = group.0;
            self.col = group.1;

            self.undo_stack
                .push(ChangeGroup(undo_row, undo_col, undo_group));
        }
    }

    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup(self.row, self.col, vec![]));
    }

    pub fn commit_change_group(&mut self) {
        if let Some(group) = self.current_change.take()
            && !group.2.is_empty()
        {
            self.undo_stack.push(group)
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
        if let Some(new_path) = path {
            let path = Path::new(&new_path);

            if let Some(dir_path) = path.parent() {
                std::fs::create_dir_all(dir_path).unwrap();
            }

            if !std::fs::exists(path).unwrap() {
                std::fs::File::create(path).unwrap().flush().unwrap();
            }

            self.path = path.canonicalize().unwrap().to_str().unwrap().to_string();
        }

        if self.path == "<scratch>" {
            tracing::error!("Unable to write to scratch file without setting a path");
            return;
        }

        if !std::fs::exists(&self.path).unwrap() {
            if let Some(dir_path) = Path::new(&self.path).parent() {
                std::fs::create_dir_all(dir_path).unwrap();
            }
            std::fs::File::create(&self.path).unwrap().flush().unwrap();
        }

        let line_ending_str = match self.line_ending_style {
            LineEnding::LF => "\n",
            LineEnding::CRLF => "\r\n",
            LineEnding::CR => "\r",
            _ => "\n", // For Mixed or None, default to LF on save
        };

        let content = self.lines.join(line_ending_str);
        if let Err(e) = std::fs::write(&self.path, content) {
            tracing::error!("Error writing file {}: {}", self.path, e);
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

    pub fn update(&mut self, theme: &Theme) {
        if let Some(s) = &mut self.ts_state {
            // When updating Tree-sitter, we must provide the text with its actual line endings.
            // Since our internal representation uses `\n`, we must convert it back.
            let line_ending_str = match self.line_ending_style {
                LineEnding::LF => "\n",
                LineEnding::CRLF => "\r\n",
                LineEnding::CR => "\r",
                _ => "\n", // Default to LF if mixed or unknown, this implies that new edits will use LF
            };
            s.update_tree_and_highlights(&self.lines.join(line_ending_str), theme);
        }
    }

    fn render(&self, mut loc: Vec2, buffer: &mut Buffer, theme: &Theme) -> Vec2 {
        let default_style = theme
            .get("ui.text")
            .unwrap_or_else(|| ContentStyle::new().with(Color::Rgb { r: 0, g: 0, b: 0 }));

        let line_style = theme
            .get("ui.linenum")
            .unwrap_or(ContentStyle::new().dark_grey());

        let mut byte_offset: usize = self
            .lines
            .iter()
            .take(self.scroll)
            .map(|l| l.len() + 1) // +1 for the newline character
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
            let mut num_line = (i + 1).to_string(); // Line numbers are 1-based
            if num_line.len() > 5 {
                num_line = num_line[0..5].to_string();
            }

            if i == self.row {
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

            render!(buffer, loc => [StyledContent::new(line_style, num_line)]);

            loc.x += gutter_width;

            if let Some(ts) = &self.ts_state {
                // 1. Get the style active at the very beginning of this line.
                let mut current_style = ts
                    .highlights
                    .range(..=byte_offset)
                    .next_back()
                    .map(|(_, style)| *style)
                    .unwrap_or(default_style);

                for (char_col, (char_byte_idx, ch)) in line.char_indices().enumerate() {
                    let absolute_byte_idx = byte_offset + char_byte_idx;

                    // 2. Check if the style changes AT this character's position.
                    if let Some(new_style) = ts.highlights.get(&absolute_byte_idx) {
                        current_style = *new_style;
                    }

                    // 3. Render the character with the now-correct style, handling horizontal scroll.
                    if char_col >= self.h_scroll {
                        let render_col = (char_col - self.h_scroll) as u16;

                        if render_col >= buffer.size().x {
                            break; // Stop rendering if we go off-screen
                        }

                        render!(buffer, loc + vec2(render_col, 0) => [StyledContent::new(current_style, ch)]);
                    }
                }
            } else {
                // Fallback for files with no syntax highlighting
                render!(buffer, loc => [ line ]);
            }

            loc.y += 1;
            byte_offset += line.len() + 1; // Account for newline
        }

        loc
    }
}

fn get_canonical_path_with_non_existent(path_str: &str) -> PathBuf {
    let path = PathBuf::from(path_str);
    let mut resolved_path = PathBuf::new();

    // Start with the base path, which is the current directory for relative paths
    // or an empty path for absolute paths.
    if !path.is_absolute() {
        resolved_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }

    // Iterate over the components of the input path.
    for component in path.components() {
        match component {
            std::path::Component::Normal(c) => {
                resolved_path.push(c);
            }
            std::path::Component::CurDir => {
                // Do nothing, as a dot doesn't change the path.
            }
            std::path::Component::ParentDir => {
                resolved_path.pop();
            }
            _ => {
                resolved_path.push(component);
            }
        }

        // After processing a component, try to canonicalize the resolved portion.
        // This handles symlinks and resolves redundant path separators.
        if resolved_path.exists()
            && let Ok(canonical) = resolved_path.canonicalize()
        {
            resolved_path = canonical;
        }
    }

    // The resolved_path now contains the final canonicalized path,
    // including any components that don't exist.
    resolved_path
}
