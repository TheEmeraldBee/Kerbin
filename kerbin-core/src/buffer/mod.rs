pub mod action;
use std::{
    io::{BufReader, BufWriter, ErrorKind, Write},
    ops::Range,
    path::{Path, PathBuf},
};

pub use action::*;

pub mod buffers;
pub use buffers::*;

pub mod tree_sitter;
use ::tree_sitter::{InputEdit, Point};
use ropey::{LineType, Rope};
use tree_sitter::*;

use ascii_forge::prelude::*;

use crate::{GrammarManager, Theme};

#[derive(Default)]
pub struct ChangeGroup(usize, Vec<Box<dyn BufferAction>>);

pub struct Cursor {
    pub range: Range<usize>,
}

pub struct TextBuffer {
    pub rope: Rope,

    pub path: String,
    pub ext: String,

    /// The byte location of the cursor
    pub cursor: usize,

    pub selection: Option<std::ops::Range<usize>>,

    current_change: Option<ChangeGroup>,

    undo_stack: Vec<ChangeGroup>,
    redo_stack: Vec<ChangeGroup>,

    pub ts_state: Option<TSState>,

    pub scroll: usize,
    pub h_scroll: usize,
}

impl TextBuffer {
    pub fn scratch() -> Self {
        Self {
            rope: Rope::new(),

            path: "<scratch>".into(),
            ext: "".into(),

            cursor: 0,

            selection: None,

            current_change: None,

            undo_stack: vec![],
            redo_stack: vec![],

            ts_state: None,

            scroll: 0,
            h_scroll: 0,
        }
    }

    pub fn open(path_str: String, grammar_manager: &mut GrammarManager, theme: &Theme) -> Self {
        let mut found_ext = "".to_string();

        let path = get_canonical_path_with_non_existent(&path_str);

        let rope = match std::fs::File::open(&path) {
            Ok(f) => {
                Rope::from_reader(BufReader::new(f)).expect("Rope should be able to read file")
            }
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::error!("{e} when opening file, {path_str}");
                }
                Rope::new()
            }
        };

        let mut ts_state = None;

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            found_ext = ext.to_string();
            ts_state = TSState::init(ext, &rope, grammar_manager, theme);
        }

        Self {
            rope,
            path: path.to_str().map(|x| x.to_string()).unwrap_or_default(),
            ext: found_ext,

            cursor: 0,

            selection: None,

            undo_stack: vec![],
            redo_stack: vec![],
            current_change: None,

            ts_state,

            scroll: 0,
            h_scroll: 0,
        }
    }

    pub fn get_edit_part(&self, byte: usize) -> (Point, usize) {
        let line_idx = self.rope.byte_to_line_idx(byte, LineType::LF_CR);
        let col = byte - line_idx;

        (Point::new(line_idx, col), byte)
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
                group.1.push(res.action)
            }

            self.redo_stack.clear();
        }

        res.success
    }

    pub fn undo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.undo_stack.pop() {
            let mut redo_group = vec![];

            let redo_cursor = self.cursor;

            for action in group.1.into_iter().rev() {
                let ActionResult { action, .. } = action.apply(self);
                redo_group.push(action);
            }

            self.cursor = group.0;

            redo_group.reverse();

            self.redo_stack.push(ChangeGroup(redo_cursor, redo_group));
        }
    }

    pub fn redo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.redo_stack.pop() {
            let mut undo_group = vec![];

            let undo_cursor = self.cursor;

            for action in group.1.into_iter() {
                let ActionResult { action, .. } = action.apply(self);
                undo_group.push(action);
            }

            self.cursor = group.0;

            self.undo_stack.push(ChangeGroup(undo_cursor, undo_group));
        }
    }

    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup(self.cursor, vec![]));
    }

    pub fn commit_change_group(&mut self) {
        if let Some(group) = self.current_change.take()
            && !group.1.is_empty()
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
            .clamp(0, self.rope.len_lines(LineType::LF_CR));

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

        self.rope
            .write_to(
                match std::fs::OpenOptions::new().write(true).open(&self.path) {
                    Ok(f) => BufWriter::new(f),
                    Err(e) => {
                        tracing::error!("Failed to write to {}: {e}", self.path);
                        return;
                    }
                },
            )
            .unwrap();
    }

    pub fn cur_line(&self) -> String {
        self.rope.line(self.cursor, LineType::LF_CR).to_string()
    }

    pub fn move_cursor(&mut self, rows: isize, cols: isize) -> bool {
        if rows == 0 && cols == 0 {
            return true;
        }

        let old_cursor_byte = self.cursor;
        let total_lines = self.rope.len_lines(LineType::LF_CR);

        let current_line_idx = self.rope.byte_to_line_idx(self.cursor, LineType::LF_CR);
        let mut current_col_byte_idx = self.cursor
            - self
                .rope
                .line_to_byte_idx(current_line_idx, LineType::LF_CR);

        let mut target_line_idx = current_line_idx.saturating_add_signed(rows);
        target_line_idx = target_line_idx.min(total_lines.saturating_sub(1)).max(0);

        let mut line_len_at_target_idx = self.rope.line(target_line_idx, LineType::LF_CR).len();
        current_col_byte_idx = current_col_byte_idx.min(line_len_at_target_idx);

        let mut temp_target_col = current_col_byte_idx as isize + cols;

        while temp_target_col < 0 && target_line_idx > 0 {
            target_line_idx = target_line_idx.saturating_sub(1);
            line_len_at_target_idx = self.rope.line(target_line_idx, LineType::LF_CR).len();
            temp_target_col += line_len_at_target_idx as isize;
            if target_line_idx == 0 && temp_target_col < 0 {
                temp_target_col = 0;
                break;
            }
        }

        while temp_target_col > line_len_at_target_idx as isize
            && target_line_idx < total_lines.saturating_sub(1)
        {
            temp_target_col -= line_len_at_target_idx as isize;
            target_line_idx = target_line_idx.saturating_add(1);
            line_len_at_target_idx = self.rope.line(target_line_idx, LineType::LF_CR).len();
        }

        let final_col_byte_idx =
            temp_target_col.max(0).min(line_len_at_target_idx as isize) as usize;

        let new_cursor_byte =
            self.rope.line_to_byte_idx(target_line_idx, LineType::LF_CR) + final_col_byte_idx;

        self.cursor = new_cursor_byte;
        self.cursor != old_cursor_byte
    }

    pub fn update(&mut self, theme: &Theme) {
        if let Some(s) = &mut self.ts_state {
            // When updating Tree-sitter, we must provide the text with its actual line endings.
            // Since our internal representation uses `\n`, we must convert it back.
            s.update_tree_and_highlights(&self.rope, theme);
        }
    }

    fn render(&self, mut loc: Vec2, buffer: &mut Buffer, theme: &Theme) -> Vec2 {
        let default_style = theme
            .get("ui.text")
            .unwrap_or_else(|| ContentStyle::new().with(Color::Rgb { r: 0, g: 0, b: 0 }));

        let line_style = theme
            .get("ui.linenum")
            .unwrap_or(ContentStyle::new().dark_grey());

        let mut byte_offset = self.rope.line_to_byte_idx(self.scroll, LineType::LF_CR);

        let row = self.rope.byte_to_line_idx(self.cursor, LineType::LF_CR);

        let gutter_width = 6;
        let start_x = loc.x;

        let mut i = self.scroll;

        for line in self
            .rope
            .lines_at(self.scroll, LineType::LF_CR)
            .take(buffer.size().y as usize)
        {
            loc.x = start_x;
            let mut num_line = (i + 1).to_string(); // Line numbers are 1-based
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

            render!(buffer, loc => [StyledContent::new(line_style, num_line)]);

            loc.x += gutter_width;

            if let Some(ts) = &self.ts_state {
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

                    if char_col >= self.h_scroll {
                        let render_col = (char_col - self.h_scroll) as u16;

                        if render_col >= buffer.size().x {
                            break;
                        }

                        render!(buffer, loc + vec2(render_col, 0) => [StyledContent::new(current_style, ch)]);
                    }
                }
            } else {
                // Fallback for files with no syntax highlighting
                render!(buffer, loc => [ line.to_string() ]);
            }

            loc.y += 1;
            byte_offset += line.len();
            i += 1;
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
