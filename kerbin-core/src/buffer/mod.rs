use std::{
    collections::{HashMap, HashSet},
    io::{self, BufReader, BufWriter, ErrorKind, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

pub mod buffer_trait;
pub use buffer_trait::*;

pub mod events;
pub use events::*;

pub mod action;
pub use action::*;

pub mod buffers;
pub use buffers::*;

pub mod systems;
use kerbin_state_machine::{StateName, StaticState};
pub use systems::*;

pub mod cursor;
pub use cursor::*;

pub mod extmark;
pub use extmark::*;

pub mod render;
pub use render::*;

pub mod text_rope_handlers;
pub use text_rope_handlers::*;

use ropey::Rope;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use crate::EVENT_BUS;

#[derive(Debug, Clone, PartialEq)]
pub enum IndentStyle {
    Tabs,
    Spaces(usize),
}

impl Default for IndentStyle {
    fn default() -> Self {
        IndentStyle::Spaces(4)
    }
}

impl IndentStyle {
    pub fn tab_string(&self) -> String {
        match self {
            Self::Tabs => "\t".to_string(),
            Self::Spaces(c) => " ".repeat(*c),
        }
    }

    pub fn tab_width(&self) -> usize {
        match self {
            IndentStyle::Tabs => 4,
            IndentStyle::Spaces(n) => *n,
        }
    }
}

/// Detect the indentation style of a rope by analysing leading-whitespace deltas.
pub fn detect_indent(rope: &Rope, default_spaces: usize) -> IndentStyle {
    for line in rope.lines() {
        if line.chars().next() == Some('\t') {
            return IndentStyle::Tabs;
        }
    }

    let mut counts = [0usize; 9];
    let mut prev_indent = 0usize;

    for line in rope.lines() {
        let s: String = line.chars().collect();
        let trimmed = s.trim_end_matches(['\n', '\r']);
        if trimmed.trim().is_empty() {
            continue;
        }

        let indent = trimmed.chars().take_while(|c| *c == ' ').count();
        if indent > prev_indent {
            let delta = indent - prev_indent;
            if delta <= 8 {
                counts[delta] += 1;
            }
        }
        prev_indent = indent;
    }

    let tab_size = counts[1..]
        .iter()
        .enumerate()
        .max_by_key(|&(_, &n)| n)
        .filter(|&(_, &n)| n > 0)
        .map(|(i, _)| i + 1)
        .unwrap_or(default_spaces);

    IndentStyle::Spaces(tab_size)
}

/// Used internally for defining a set of actions that were applied together as a single undo/redo unit
#[derive(Default)]
pub struct ChangeGroup(Vec<Cursor>, Vec<Box<dyn BufferAction>>);

/// The core storage of an open text buffer inside of the editor
pub struct TextBuffer {
    pub dirty: bool,
    /// Index into `undo_stack` at the last save; used to compute `dirty`
    pub save_point: usize,
    pub version: u128,
    pub changed: Option<SystemTime>,

    pub(crate) rope: Rope,

    pub path: String,
    pub ext: String,
    pub filetype: Option<String>,
    pub indent_style: IndentStyle,

    pub cursors: Vec<Cursor>,
    pub primary_cursor: usize,

    pub flags: HashSet<&'static str>,
    pub states: HashMap<String, Box<dyn StateName>>,

    /// Pending byte-range edits to be flushed to `renderer` at end of frame
    pub byte_changes: Vec<[((usize, usize), usize); 3]>,

    pub current_change: Option<ChangeGroup>,

    /// Past changes that can be undone
    pub undo_stack: Vec<ChangeGroup>,
    /// Undone changes that can be redone
    pub redo_stack: Vec<ChangeGroup>,

    pub renderer: BufferRenderer,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            dirty: false,
            save_point: 0,
            version: 0,
            changed: None,

            rope: Rope::new(),

            path: "<scratch>".into(),
            ext: "".into(),
            filetype: None,
            indent_style: IndentStyle::default(),

            cursors: vec![Cursor::default()],
            primary_cursor: 0,

            flags: HashSet::default(),
            states: HashMap::default(),

            byte_changes: vec![],

            current_change: None,

            undo_stack: vec![],
            redo_stack: vec![],

            renderer: BufferRenderer::default(),
        }
    }
}

impl TextBuffer {
    /// Creates an in-memory buffer with no associated file path
    pub fn scratch() -> Self {
        Self::default()
    }

    /// Opens a file with the provided path, loading its content into the buffer
    pub fn open(path_str: String, default_tab_unit: usize) -> io::Result<Self> {
        let mut found_ext = "".to_string();

        let path = get_canonical_path_with_non_existent(&path_str);

        let mut changed = None;

        let rope = match std::fs::File::open(&path) {
            Ok(f) => {
                changed = f.metadata()?.modified().ok();
                Rope::from_reader(BufReader::new(f))?
            }
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    tracing::error!("{e} when opening file, {path_str}");
                }
                Rope::new()
            }
        };

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            found_ext = ext.to_string();
        }

        let indent_style = detect_indent(&rope, default_tab_unit);

        Ok(Self {
            save_point: 0,

            changed,

            rope,
            path: path.to_str().map(|x| x.to_string()).unwrap_or_default(),
            ext: found_ext,
            indent_style,

            ..Default::default()
        })
    }

    pub fn version(&self) -> &u128 {
        &self.version
    }

    /// Adds an extmark, stamping it with the current file version
    pub fn add_extmark(&mut self, builder: ExtmarkBuilder) -> u64 {
        let file_ver = self.version;

        self.renderer.add_extmark(file_ver, builder)
    }

    pub fn set_state<T: StateName + StaticState>(&mut self, state: T) {
        self.states
            .insert(state.name(), Box::new(Arc::new(RwLock::new(state))));
    }

    pub fn has_state<T: StateName + StaticState>(&self) -> bool {
        self.states.contains_key(&T::static_name())
    }

    /// Inserts the state produced by `func` only if the type is not already present
    pub fn maybe_insert_state<T: StateName + StaticState>(&mut self, func: impl FnOnce() -> T) {
        if !self.has_state::<T>() {
            self.set_state(func());
        }
    }

    pub async fn get_or_insert_state<T: StateName + StaticState>(
        &mut self,
        func: impl FnOnce() -> T,
    ) -> OwnedRwLockReadGuard<T> {
        self.maybe_insert_state(func);
        self.get_state::<T>().await.unwrap()
    }

    pub async fn get_or_insert_state_mut<T: StateName + StaticState>(
        &mut self,
        func: impl FnOnce() -> T,
    ) -> OwnedRwLockWriteGuard<T> {
        self.maybe_insert_state(func);
        self.get_state_mut::<T>().await.unwrap()
    }

    pub async fn get_state_mut<T: StateName + StaticState>(
        &mut self,
    ) -> Option<OwnedRwLockWriteGuard<T>> {
        if let Some(s) = self
            .states
            .get(&T::static_name())
            .and_then(|x| x.downcast())
        {
            Some(s.clone().write_owned().await)
        } else {
            None
        }
    }

    pub async fn get_state<T: StateName + StaticState>(&self) -> Option<OwnedRwLockReadGuard<T>> {
        if let Some(s) = self
            .states
            .get(&T::static_name())
            .and_then(|x| x.downcast())
        {
            Some(s.clone().read_owned().await)
        } else {
            None
        }
    }

    /// Given a byte offset, returns a tuple containing the `((line_idx, col_idx), byte_idx)`
    pub fn get_edit_part(&self, byte: usize) -> ((usize, usize), usize) {
        let line_idx = self.byte_to_line_clamped(byte);
        let col = byte - self.line_to_byte_clamped(line_idx);

        ((line_idx, col), byte)
    }

    pub fn register_input_edit(
        &mut self,
        start: ((usize, usize), usize),
        old_end: ((usize, usize), usize),
        new_end: ((usize, usize), usize),
    ) {
        self.byte_changes.push([start, old_end, new_end]);
    }

    pub fn action(&mut self, action: impl BufferAction) -> bool {
        if self.current_change.is_none() {
            self.start_change_group();
        }

        let res = action.apply(self);

        if res.success {
            self.dirty = true;

            if let Some(group) = self.current_change.as_mut() {
                group.1.push(res.action)
            }

            self.redo_stack.clear();
        }

        res.success
    }

    /// Creates a new cursor at the same location as the current primary cursor
    pub fn create_cursor(&mut self) {
        self.cursors.push(self.primary_cursor().clone());
        self.primary_cursor = self.cursors.len() - 1;
    }

    /// Removes all cursors from the buffer except for the current primary cursor
    pub fn drop_other_cursors(&mut self) {
        let cursor = self.cursors.remove(self.primary_cursor);
        self.primary_cursor = 0;
        self.cursors.clear();

        self.cursors.push(cursor);
    }

    pub fn drop_primary_cursor(&mut self) {
        if self.cursors.len() <= 1 {
            return;
        }

        self.cursors.remove(self.primary_cursor);

        self.primary_cursor = self
            .primary_cursor
            .saturating_sub(1)
            .clamp(0, self.cursors.len() - 1);
    }

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

            if self.undo_stack.len() == self.save_point {
                // We have undone back to the save point. The buffer is now clean.
                self.dirty = false;
            } else {
                // We are either past the save point or haven't reached it. The buffer is dirty.
                self.dirty = true;
            }

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

            if self.undo_stack.len() + 1 > self.save_point {
                // We have redone past the save point. The buffer is now dirty.
                self.dirty = true;
            } else {
                // Push before the save-point check so the lengths are accurate
                self.undo_stack.push(ChangeGroup(undo_cursor, undo_group));

                self.dirty = self.undo_stack.len() != self.save_point;
                return;
            }

            self.undo_stack.push(ChangeGroup(undo_cursor, undo_group));
        }
    }

    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup(self.cursors.clone(), vec![]));
    }

    /// Commits the current `ChangeGroup` to the undo stack, if it's not empty
    pub fn commit_change_group(&mut self) {
        if let Some(group) = self.current_change.take()
            && !group.1.is_empty()
        {
            self.undo_stack.push(group)
        }
    }

    pub async fn write_file(&mut self, path: Option<String>) -> Result<(), std::io::Error> {
        if let Some(new_path) = path {
            let path = Path::new(&new_path);

            if let Some(dir_path) = path.parent() {
                std::fs::create_dir_all(dir_path)?;
            }

            if !std::fs::exists(path)? {
                std::fs::File::create(path)?.flush()?;
            }

            self.path = path
                .canonicalize()?
                .to_str()
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "path contains non-UTF-8 characters",
                    )
                })?
                .to_string();
        }

        if self.path.starts_with("<") && self.path.ends_with(">") {
            tracing::error!("Cannot write to special buffer without setting new path");
            return Ok(());
        }

        EVENT_BUS
            .emit(SaveEvent {
                path: self.path.clone(),
            })
            .await;

        if !std::fs::exists(&self.path)? {
            if let Some(dir_path) = Path::new(&self.path).parent() {
                std::fs::create_dir_all(dir_path)?;
            }
            std::fs::File::create(&self.path)?.flush()?;
        }

        self.dirty = false;

        let write_result = self.rope.write_to(
            match std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&self.path)
            {
                Ok(f) => BufWriter::new(f),
                Err(e) => {
                    tracing::error!("Failed to write to {}: {e}", self.path);
                    return Err(e);
                }
            },
        );

        if let Err(e) = write_result {
            tracing::error!("Failed to write rope content to file: {:?}", e);
            return Err(std::io::Error::other(e.to_string()));
        }

        self.dirty = false;

        self.save_point = self.undo_stack.len();

        match std::fs::metadata(&self.path) {
            Ok(metadata) => {
                self.changed = metadata.modified().ok();
            }
            Err(e) => {
                tracing::error!(
                    "Failed to get metadata after writing file {}: {e}",
                    self.path
                );
                self.changed = None;
            }
        }

        Ok(())
    }

    /// Writes the buffer contents to disk and updates dirty/save_point/changed,
    /// without emitting SaveEvent. Used after format-on-save edits are applied.
    pub fn write_file_bare(&mut self) -> Result<(), std::io::Error> {
        if !std::fs::exists(&self.path)? {
            if let Some(dir_path) = Path::new(&self.path).parent() {
                std::fs::create_dir_all(dir_path)?;
            }
            std::fs::File::create(&self.path)?.flush()?;
        }

        let write_result = self.rope.write_to(
            match std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&self.path)
            {
                Ok(f) => BufWriter::new(f),
                Err(e) => return Err(e),
            },
        );

        if let Err(e) = write_result {
            return Err(std::io::Error::other(e.to_string()));
        }

        self.dirty = false;
        self.save_point = self.undo_stack.len();

        match std::fs::metadata(&self.path) {
            Ok(metadata) => self.changed = metadata.modified().ok(),
            Err(_) => self.changed = None,
        }

        Ok(())
    }

    /// Scrolls the viewport by `lines` rows.  The update loop will clamp the cursor
    /// into the new visible area (with scroll padding) on the next frame.
    pub fn scroll_lines(&mut self, lines: isize) {
        let max_scroll = self.len_lines().saturating_sub(1);
        let new_scroll =
            (self.renderer.byte_scroll as isize + lines).clamp(0, max_scroll as isize) as usize;
        self.renderer.byte_scroll = new_scroll;
        self.renderer.visual_scroll = 0;
        self.renderer.cursor_drag = true;
    }

    pub fn move_bytes(&mut self, bytes: isize, extend_selection: bool) -> bool {
        if bytes == 0 {
            return false;
        }

        let current_cursor = self.primary_cursor();
        let current_caret_byte = current_cursor.get_cursor_byte();

        let new_caret_byte =
            (current_caret_byte as isize + bytes).clamp(0, self.len() as isize) as usize;

        let cursor_mut = self.primary_cursor_mut();

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
        new_caret_byte != current_caret_byte
    }

    pub fn move_lines(&mut self, rows: isize, extend_selection: bool, tab_w: usize) -> bool {
        if rows == 0 {
            return false;
        }

        let current_caret_byte = self.primary_cursor().get_cursor_byte();
        let current_line_idx = self.byte_to_line_clamped(current_caret_byte);
        let line_start_byte = self.line_to_byte_clamped(current_line_idx);

        let line_prefix = self.slice_to_string(line_start_byte, current_caret_byte).unwrap_or_default();
        let current_visual_col = byte_offset_to_display_col(&line_prefix, line_prefix.len(), tab_w);

        let total_lines = self.len_lines();
        let target_line_idx = current_line_idx
            .saturating_add_signed(rows)
            .clamp(0, total_lines.saturating_sub(1));

        let target_line_text = self.line_clamped(target_line_idx).to_string();
        let target_byte_offset = display_col_to_byte_offset(&target_line_text, current_visual_col, tab_w);
        let new_caret_byte = self.line_to_byte_clamped(target_line_idx) + target_byte_offset;

        let cursor_mut = self.primary_cursor_mut();

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
        new_caret_byte != current_caret_byte
    }

    pub fn move_chars(&mut self, chars: isize, extend_selection: bool) -> bool {
        if chars == 0 {
            return false;
        }

        let current_cursor = self.primary_cursor().clone();
        let current_caret_char_idx = self.byte_to_char_clamped(current_cursor.get_cursor_byte());

        let new_caret_char_idx =
            (current_caret_char_idx as isize + chars).clamp(0, self.len_chars() as isize) as usize;

        let new_caret_byte = self.char_to_byte_clamped(new_caret_char_idx);

        let cursor_mut = self.primary_cursor_mut();

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
        new_caret_byte != current_cursor.get_cursor_byte()
    }

    pub fn merge_overlapping_cursors(&mut self) {
        if self.cursors.len() <= 1 {
            return;
        }

        let mut i = 0;
        while i < self.cursors.len() {
            let mut j = i + 1;
            while j < self.cursors.len() {
                let (cursor1, cursor2) = if i < j {
                    let split = self.cursors.split_at_mut(j);
                    (&mut split.0[i], &mut split.1[0])
                } else {
                    let split = self.cursors.split_at_mut(i);
                    (&mut split.1[0], &mut split.0[j])
                };

                let sel1 = cursor1.sel();
                let sel2 = cursor2.sel();

                let overlaps = sel1.start() <= sel2.end() && sel2.start() <= sel1.end();

                if overlaps {
                    let start = (*sel1.start()).min(*sel2.start());
                    let end = (*sel1.end()).max(*sel2.end());
                    cursor1.set_sel(start..=end);
                    self.cursors.remove(j);
                    if self.primary_cursor == j {
                        self.primary_cursor = i;
                    } else if self.primary_cursor > j {
                        self.primary_cursor -= 1;
                    }
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }

    pub fn post_update(&mut self) {
        self.renderer
            .process_byte_changes(*self.version(), &self.byte_changes);
    }

    pub fn update_cleanup(&mut self) {
        self.merge_overlapping_cursors();

        self.byte_changes.clear();
    }

    pub fn insert(&mut self, byte: usize, text: &str) {
        self.rope.insert(self.rope.byte_to_char(byte), text);
    }

    pub fn remove_range(&mut self, range: std::ops::Range<usize>) {
        self.rope
            .remove(self.rope.byte_to_char(range.start)..self.rope.byte_to_char(range.end));
    }

    pub fn get_rope(&self) -> &Rope {
        &self.rope
    }
}

/// Computes the canonicalized path even if parts do not exist
pub fn get_canonical_path_with_non_existent(path_str: &str) -> PathBuf {
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
