pub mod action;
use std::{
    io::{BufReader, BufWriter, ErrorKind, Write},
    ops::RangeInclusive,
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

use crate::{ContentStyleExt, GrammarManager, Theme};

#[derive(Default)]
pub struct ChangeGroup(Vec<Cursor>, Vec<Box<dyn BufferAction>>);

#[derive(Clone, Debug)]
pub struct Cursor {
    at_start: bool,
    sel: RangeInclusive<usize>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            at_start: false,
            sel: 0..=0,
        }
    }
}

impl Cursor {
    /// Returns the byte position of where the actual cursor would be
    /// Can be at end or beginning of selection
    pub fn get_cursor_byte(&self) -> usize {
        match self.at_start {
            true => *self.sel.start(),
            false => *self.sel.end(),
        }
    }

    /// Returns which end of the selection the cursor is
    pub fn at_start(&self) -> bool {
        self.at_start
    }

    /// Sets where the actual cursor is based on the selection
    pub fn set_at_start(&mut self, at_start: bool) {
        self.at_start = at_start
    }

    /// Returns a range of the selection for this cursor
    pub fn sel(&self) -> &RangeInclusive<usize> {
        &self.sel
    }

    /// Sets the range of selection
    pub fn set_sel(&mut self, range: RangeInclusive<usize>) {
        self.sel = range;
    }

    /// Collapses the selection into the location of the cursor
    pub fn collapse_sel(&mut self) {
        match self.at_start {
            true => self.sel = *self.sel.start()..=*self.sel.start(),
            false => self.sel = *self.sel.end()..=*self.sel.end(),
        }
    }
}

pub struct TextBuffer {
    pub rope: Rope,

    pub path: String,
    pub ext: String,

    pub cursors: Vec<Cursor>,
    pub primary_cursor: usize,

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

            cursors: vec![Cursor::default()],
            primary_cursor: 0,

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

            cursors: vec![Cursor::default()],
            primary_cursor: 0,

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

    /// Creates a cursor at the same location as the current cursor
    /// Sets primary cursor to this new cursor
    pub fn create_cursor(&mut self) {
        self.cursors.push(self.primary_cursor().clone());
        self.primary_cursor = self.cursors.len() - 1;
    }

    /// Removes all cursors other than the current primary cursor
    pub fn drop_other_cursors(&mut self) {
        let cursor = self.cursors.remove(self.primary_cursor);
        self.primary_cursor = 0;
        self.cursors.clear();

        self.cursors.push(cursor);
    }

    /// Will remove the current cursor unless it is the only cursor that exists
    pub fn drop_primary_cursor(&mut self) {
        if self.cursors.is_empty() {
            return;
        }

        self.cursors.remove(self.primary_cursor);

        self.primary_cursor = self
            .primary_cursor
            .saturating_sub(1)
            .clamp(0, self.cursors.len() - 1);
    }

    /// Will change the currently selected cursor by an offset
    /// will **not** wrap or go out of bounds
    pub fn change_cursor(&mut self, offset: isize) {
        self.primary_cursor = self
            .primary_cursor
            .saturating_add_signed(offset)
            .clamp(0, self.cursors.len() - 1);
    }

    pub fn primary_cursor(&self) -> &Cursor {
        &self.cursors[self.primary_cursor]
    }

    pub fn primary_cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursors[self.primary_cursor]
    }

    pub fn undo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.undo_stack.pop() {
            let mut redo_group = vec![];

            let redo_cursor = self.cursors.clone();

            for action in group.1.into_iter().rev() {
                let ActionResult { action, .. } = action.apply(self);
                redo_group.push(action);
            }

            self.cursors = group.0;

            redo_group.reverse();

            self.redo_stack.push(ChangeGroup(redo_cursor, redo_group));
        }
    }

    pub fn redo(&mut self) {
        self.commit_change_group();
        if let Some(group) = self.redo_stack.pop() {
            let mut undo_group = vec![];

            let undo_cursor = self.cursors.clone();

            for action in group.1.into_iter() {
                let ActionResult { action, .. } = action.apply(self);
                undo_group.push(action);
            }

            self.cursors = group.0;

            self.undo_stack.push(ChangeGroup(undo_cursor, undo_group));
        }
    }

    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup(self.cursors.clone(), vec![]));
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

    pub fn move_cursor(&mut self, rows: isize, cols: isize, extend_selection: bool) -> bool {
        if rows == 0 && cols == 0 {
            return true;
        }

        let mut moved_any = false;
        let total_lines = self.rope.len_lines(LineType::LF_CR);

        let current_cursor = self.primary_cursor();
        let current_caret_byte = current_cursor.get_cursor_byte();

        let current_line_idx = self
            .rope
            .byte_to_line_idx(current_caret_byte, LineType::LF_CR);
        let line_start_byte = self
            .rope
            .line_to_byte_idx(current_line_idx, LineType::LF_CR);
        let mut current_col_byte_idx = current_caret_byte - line_start_byte;

        let mut target_line_idx = current_line_idx.saturating_add_signed(rows);
        target_line_idx = target_line_idx.min(total_lines.saturating_sub(1)).max(0);

        let mut line_len_at_target_idx_bytes =
            self.rope.line(target_line_idx, LineType::LF_CR).len();

        current_col_byte_idx = current_col_byte_idx.min(line_len_at_target_idx_bytes);

        let mut temp_target_col_byte = current_col_byte_idx as isize + cols;

        // Handle horizontal wrap-around (move to previous/next lines)
        while temp_target_col_byte < 0 && target_line_idx > 0 {
            target_line_idx = target_line_idx.saturating_sub(1);
            line_len_at_target_idx_bytes = self.rope.line(target_line_idx, LineType::LF_CR).len();
            temp_target_col_byte += line_len_at_target_idx_bytes as isize;
            if target_line_idx == 0 && temp_target_col_byte < 0 {
                temp_target_col_byte = 0;
                break;
            }
        }

        while temp_target_col_byte > line_len_at_target_idx_bytes as isize
            && target_line_idx < total_lines.saturating_sub(1)
        {
            temp_target_col_byte -= line_len_at_target_idx_bytes as isize;
            target_line_idx = target_line_idx.saturating_add(1);
            line_len_at_target_idx_bytes = self.rope.line(target_line_idx, LineType::LF_CR).len();
        }

        let final_col_byte_idx = temp_target_col_byte
            .max(0)
            .min(line_len_at_target_idx_bytes as isize) as usize;

        let new_caret_byte =
            self.rope.line_to_byte_idx(target_line_idx, LineType::LF_CR) + final_col_byte_idx;

        let cursor_mut = self.primary_cursor_mut();

        if new_caret_byte != current_caret_byte {
            moved_any = true;
        }

        if extend_selection {
            let anchor_byte = if cursor_mut.at_start {
                *cursor_mut.sel.end()
            } else {
                *cursor_mut.sel.start()
            };

            let start = anchor_byte.min(new_caret_byte);
            let end = anchor_byte.max(new_caret_byte);
            cursor_mut.set_sel(start..=end);
            cursor_mut.set_at_start(new_caret_byte < anchor_byte);
        } else {
            cursor_mut.set_sel(new_caret_byte..=new_caret_byte);
            cursor_mut.set_at_start(false);
        }
        moved_any
    }

    pub fn update(&mut self, theme: &Theme) {
        if let Some(s) = &mut self.ts_state {
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

        let sel_style = theme.get("ui.selection");

        let mut byte_offset = self.rope.line_to_byte_idx(self.scroll, LineType::LF_CR);

        let row = self
            .rope
            .byte_to_line_idx(self.primary_cursor().get_cursor_byte(), LineType::LF_CR);

        let gutter_width = 6;
        let start_x = loc.x;

        let mut i = self.scroll;

        for line in self
            .rope
            .lines_at(self.scroll, LineType::LF_CR)
            .take(buffer.size().y as usize)
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

                    let mut in_selection = false;
                    for cursor in &self.cursors {
                        if cursor.sel().contains(&absolute_byte_idx) {
                            in_selection = true;
                            break;
                        }
                    }

                    let resulting_style = match in_selection {
                        false => current_style,
                        true => sel_style
                            .map(|x| x.combined_with(&current_style))
                            .unwrap_or(current_style.on_grey()),
                    };

                    if char_col >= self.h_scroll {
                        let render_col = (char_col - self.h_scroll) as u16;

                        if render_col >= buffer.size().x {
                            break;
                        }

                        render!(buffer, loc + vec2(render_col, 0) => [StyledContent::new(resulting_style, ch)]);
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

    if !path.is_absolute() {
        resolved_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }

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

        if resolved_path.exists()
            && let Ok(canonical) = resolved_path.canonicalize()
        {
            resolved_path = canonical;
        }
    }

    resolved_path
}
