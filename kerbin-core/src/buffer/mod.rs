use std::{
    collections::{HashMap, HashSet},
    io::{self, BufReader, BufWriter, ErrorKind, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

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

/// Used internally for defining a set of actions that were applied together as a single undo/redo unit.
///
/// A `ChangeGroup` stores the state of cursors *before* the actions were applied,
/// whether the buffer was dirty or not in the change,
/// and a list of `BufferAction` inverses to reverse the changes.
#[derive(Default)]
pub struct ChangeGroup(Vec<Cursor>, Vec<Box<dyn BufferAction>>);

/// The core storage of an open text buffer inside of the editor.
///
/// `TextBuffer` is responsible for storing file content (`ropey::Rope`),
/// managing file metadata (path, extension), tracking multiple cursors,
/// handling undo/redo, and managing scroll positions for rendering.
pub struct TextBuffer {
    /// A marker for the text buffer that marks if it is unsaved
    /// Used for commands that may do file operations or similar
    pub dirty: bool,

    /// An optional index into `undo_stack` that marks the point
    /// where the content matches the last file save on disk.
    pub save_point: usize,

    /// A number representing the "version" of an edit,
    /// is incremented for any action applied,
    /// including undo and redo
    pub version: u128,

    /// The last stored time that the file was changed. None if the file didn't exist, or if
    /// reading it failed. Used for commands that write to file, to check if external changes exist
    pub changed: Option<SystemTime>,

    /// Internal storage of the text itself using `ropey::Rope`.
    ///
    /// Changes to the `Rope` should primarily be made through `BufferAction`s
    /// to correctly integrate with undo/redo and syntax highlighting systems.
    pub(crate) rope: Rope,

    /// The absolute, canonical path of the file associated with this buffer.
    /// Used for saving and identifying the file.
    pub path: String,

    /// The file extension (e.g., "rs", "txt") derived from the `path`.
    /// Used for determining syntax highlighting and other file-type-specific behaviors.
    pub ext: String,

    /// A vector of all active `Cursor`s in this buffer.
    /// Supports multicursor editing.
    pub cursors: Vec<Cursor>,

    /// The index within the `cursors` vector that identifies the primary,
    /// or active, cursor.
    pub primary_cursor: usize,

    /// A set of flags that may be set on a buffer
    pub flags: HashSet<&'static str>,

    /// A set of states stored within a buffer
    /// Used for tying states to a buffer specifically
    pub states: HashMap<String, Box<dyn StateName>>,

    /// A list of data that marks byte changes applied to the rope.
    /// Each entry is an array of three `((row, col), byte_idx)` tuples:
    /// `[0]` is the start position of the edit.
    /// `[1]` is the previous ending position of the edit.
    /// `[2]` is the new ending position of the edit.
    /// This is used for systems like incremental syntax highlighting updates.
    pub byte_changes: Vec<[((usize, usize), usize); 3]>,

    /// An optional `ChangeGroup` currently being built.
    /// Actions are added to this group until `commit_change_group` is called.
    pub current_change: Option<ChangeGroup>,

    /// A stack of `ChangeGroup`s representing past changes that can be undone.
    pub undo_stack: Vec<ChangeGroup>,
    /// A stack of `ChangeGroup`s representing undone changes that can be redone.
    pub redo_stack: Vec<ChangeGroup>,

    /// Stores the current render state of the buffer
    /// used for handling the rendering of the buffer
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
    /// Creates a new "scratch" file, which is an in-memory, unsavable buffer.
    ///
    /// Scratch buffers are typically used for new, unsaved files or as a default
    /// buffer when the editor starts without opening a specific file.
    ///
    /// # Returns
    ///
    /// A new `TextBuffer` instance representing a scratch file.
    pub fn scratch() -> Self {
        Self::default()
    }

    /// Opens a file with the provided path, loading its content into the buffer.
    ///
    /// The `path_str` can be absolute or relative. It will be canonicalized,
    /// even if the file does not yet exist. This method also handles
    /// extracting the file extension, initializing cursors, and reading file content.
    /// If the file does not exist, an empty buffer is created.
    pub fn open(path_str: String) -> io::Result<Self> {
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

        Ok(Self {
            save_point: 0,

            changed,

            rope,
            path: path.to_str().map(|x| x.to_string()).unwrap_or_default(),
            ext: found_ext,

            ..Default::default()
        })
    }

    /// Returns the current version of the buffer
    pub fn version(&self) -> &u128 {
        &self.version
    }

    /// Adds an extmark to the renderer, handling file version for you
    pub fn add_extmark(&mut self, builder: ExtmarkBuilder) -> u64 {
        let file_ver = self.version;

        self.renderer.add_extmark(file_ver, builder)
    }

    /// Inserts a state into the buffer, replacing the value if it exists
    pub fn set_state<T: StateName + StaticState>(&mut self, state: T) {
        self.states
            .insert(state.name(), Box::new(Arc::new(RwLock::new(state))));
    }

    /// Returns whether the state is within the buffer or not
    pub fn has_state<T: StateName + StaticState>(&self) -> bool {
        self.states.contains_key(&T::static_name())
    }

    /// Given a function, will do nothing if state exists, or inserts it if it doesn't
    pub fn maybe_insert_state<T: StateName + StaticState>(&mut self, func: impl FnOnce() -> T) {
        if !self.has_state::<T>() {
            self.set_state(func());
        }
    }

    /// Retrieves state from buffer or inserts if non-existent
    pub async fn get_or_insert_state<T: StateName + StaticState>(
        &mut self,
        func: impl FnOnce() -> T,
    ) -> OwnedRwLockReadGuard<T> {
        self.maybe_insert_state(func);
        self.get_state::<T>().await.unwrap()
    }

    /// Retrieves state mutably from buffer or inserts if non-existent
    pub async fn get_or_insert_state_mut<T: StateName + StaticState>(
        &mut self,
        func: impl FnOnce() -> T,
    ) -> OwnedRwLockWriteGuard<T> {
        self.maybe_insert_state(func);
        self.get_state_mut::<T>().await.unwrap()
    }

    /// Retrieves a state from the internal storage, returning None if non-existent
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

    /// Retrieves a state from the internal storage, returning None if non-existent
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

    /// Given a byte offset, returns a tuple containing the `((line_idx, col_idx), byte_idx)`.
    ///
    /// This format is convenient for registering edits within the `byte_changes` vector,
    /// particularly for external tools like Tree-sitter.
    pub fn get_edit_part(&self, byte: usize) -> ((usize, usize), usize) {
        let line_idx = self.byte_to_line_clamped(byte);
        let col = byte - self.line_to_byte_clamped(line_idx);

        ((line_idx, col), byte)
    }

    /// Registers an edit with the buffer for tracking changes.
    ///
    /// This method stores the start, old end, and new end positions of an edit.
    /// This information is crucial for systems that need to react to buffer changes,
    /// such as syntax highlighting or language server protocols.
    pub fn register_input_edit(
        &mut self,
        start: ((usize, usize), usize),
        old_end: ((usize, usize), usize),
        new_end: ((usize, usize), usize),
    ) {
        self.byte_changes.push([start, old_end, new_end]);
    }

    /// Applies a given `BufferAction` to the editor.
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

    /// Creates a new cursor at the same location as the current primary cursor.
    ///
    /// The newly created cursor becomes the new primary cursor.
    pub fn create_cursor(&mut self) {
        self.cursors.push(self.primary_cursor().clone());
        self.primary_cursor = self.cursors.len() - 1;
    }

    /// Removes all cursors from the buffer except for the current primary cursor.
    ///
    /// After this operation, only one cursor will remain, and it will be set
    /// as the primary cursor at index 0.
    pub fn drop_other_cursors(&mut self) {
        let cursor = self.cursors.remove(self.primary_cursor);
        self.primary_cursor = 0;
        self.cursors.clear();

        self.cursors.push(cursor);
    }

    /// Removes the current primary cursor from the buffer.
    ///
    /// If there is only one cursor, this action does nothing, as a buffer
    /// must always have at least one cursor. If multiple cursors exist,
    /// the primary cursor is removed, and the `primary_cursor` index
    /// is adjusted to point to a valid remaining cursor.
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

    /// Changes the currently selected primary cursor by an offset.
    ///
    /// The `primary_cursor` index will be clamped to remain within the
    /// valid range of `0` to `self.cursors.len() - 1` and will not wrap.
    pub fn change_cursor(&mut self, offset: isize) {
        self.primary_cursor = self
            .primary_cursor
            .saturating_add_signed(offset)
            .clamp(0, self.cursors.len() - 1);
    }

    /// Returns an immutable reference to the current primary cursor of the buffer.
    pub fn primary_cursor(&self) -> &Cursor {
        &self.cursors[self.primary_cursor]
    }

    /// Returns a mutable reference to the current primary cursor of the buffer.
    pub fn primary_cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursors[self.primary_cursor]
    }

    /// Applies the undo operation of the last `ChangeGroup` on the undo stack.
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

    /// Applies the redo operation from the redo stack.
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
                // Push the redone group to the undo stack *before* checking the save point logic
                self.undo_stack.push(ChangeGroup(undo_cursor, undo_group));

                if self.undo_stack.len() == self.save_point {
                    // The current state is the save point. It is clean.
                    self.dirty = false;
                } else {
                    self.dirty = true; // Any other state is dirty.
                }
                return; // Return early since the push happened inside the if block
            }

            self.undo_stack.push(ChangeGroup(undo_cursor, undo_group));
        }
    }

    /// Starts a new change group for recording subsequent actions.
    ///
    /// Any pending actions in `current_change` are first committed to the undo stack.
    /// A new empty `ChangeGroup` is then initiated.
    pub fn start_change_group(&mut self) {
        self.commit_change_group();
        self.current_change = Some(ChangeGroup(self.cursors.clone(), vec![]));
    }

    /// Commits the current `ChangeGroup` to the undo stack, if it's not empty.
    ///
    /// If there is an active `current_change` with recorded actions, it is moved
    /// to the `undo_stack`. If `current_change` is empty or `None`, this does nothing.
    pub fn commit_change_group(&mut self) {
        if let Some(group) = self.current_change.take()
            && !group.1.is_empty()
        {
            self.undo_stack.push(group)
        }
    }

    /// Writes the buffer's content to a file on disk.
    ///
    /// Handles directory creation and ensures the file exists.
    /// A special buffer (starts with `<` and ends with `>`) cannot be written without providing a new path.
    pub async fn write_file(&mut self, path: Option<String>) {
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

        if self.path.starts_with("<") && self.path.ends_with(">") {
            tracing::error!("Cannot write to special buffer without setting new path");
            return;
        }

        EVENT_BUS
            .emit(SaveEvent {
                path: self.path.clone(),
            })
            .await;

        if !std::fs::exists(&self.path).unwrap() {
            if let Some(dir_path) = Path::new(&self.path).parent() {
                std::fs::create_dir_all(dir_path).unwrap();
            }
            std::fs::File::create(&self.path).unwrap().flush().unwrap();
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
                    return;
                }
            },
        );

        if write_result.is_err() {
            tracing::error!(
                "Failed to write rope content to file: {:?}",
                write_result.err()
            );
            return;
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
    }

    /// Moves the primary cursor by a given number of bytes.
    ///
    /// This function directly adjusts the cursor's position by `bytes` within the buffer's
    /// content, clamping the new position to be within the valid range of the `Rope`.
    /// It can optionally extend the current selection.
    pub fn move_bytes(&mut self, bytes: isize, extend_selection: bool) -> bool {
        if bytes == 0 {
            return false;
        }

        let current_cursor = self.primary_cursor();
        let current_caret_byte = current_cursor.get_cursor_byte();

        let new_caret_byte =
            (current_caret_byte as isize + bytes).clamp(0, self.len_bytes() as isize) as usize;

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

    /// Moves the primary cursor by a given number of lines.
    ///
    /// This function adjusts the cursor's line position by `rows`, attempting to maintain
    /// the current column position. The new line position is clamped to be within
    /// the valid range of lines in the `Rope`. It can optionally extend the current selection.
    pub fn move_lines(&mut self, rows: isize, extend_selection: bool) -> bool {
        if rows == 0 {
            return false;
        }

        let current_cursor = self.primary_cursor();
        let current_caret_byte = current_cursor.get_cursor_byte();

        let current_line_idx = self.byte_to_line_clamped(current_caret_byte);
        let line_start_byte = self.line_to_byte_clamped(current_line_idx);
        let current_col_char_idx = self.byte_to_char_clamped(current_caret_byte)
            - self.byte_to_char_clamped(line_start_byte);

        let total_lines = self.len_lines();
        let mut target_line_idx = current_line_idx.saturating_add_signed(rows);
        target_line_idx = target_line_idx.clamp(0, total_lines.saturating_sub(1));

        let line_slice = self.line_clamped(target_line_idx);
        let line_len_with_ending = line_slice.len_chars();
        let endline_text = line_slice
            .chars_at(line_slice.char_to_byte_idx(line_len_with_ending.saturating_sub(2)))
            .collect::<String>();

        let line_ending_len = if endline_text.ends_with("\r\n") {
            2
        } else if endline_text.ends_with("\n") || endline_text.ends_with("\r") {
            1
        } else {
            0
        };
        let line_len_without_ending = line_len_with_ending - line_ending_len;

        let final_col_char_idx = current_col_char_idx.min(line_len_without_ending);

        let new_caret_byte = self.line_to_byte_clamped(target_line_idx)
            + self
                .line_clamped(target_line_idx)
                .char_to_byte_idx(final_col_char_idx);

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

    /// Moves the primary cursor by a given number of characters.
    ///
    /// This function adjusts the cursor's character position by `chars` within the buffer's
    /// content, clamping the new position to be within the valid range of the `Rope`.
    /// It handles multi-byte characters correctly. It can optionally extend the current selection.
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

    /// Merges any overlapping cursors in the `cursors` list.
    ///
    /// If multiple cursors select overlapping regions of text, they are combined
    /// into a single cursor whose selection encompasses the merged region.
    /// The `primary_cursor` index is adjusted if a cursor before it is removed.
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
                    if self.primary_cursor >= j {
                        self.primary_cursor = self.primary_cursor.saturating_sub(1);
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

    /// Inserts text at the specified byte offset.
    pub fn insert(&mut self, byte: usize, text: &str) {
        self.rope.insert(byte, text);
    }

    /// Removes text within the specified byte range.
    pub fn remove_range(&mut self, range: std::ops::Range<usize>) {
        self.rope.remove(range);
    }

    pub fn get_rope(&self) -> &Rope {
        &self.rope
    }
}

/// Computes the canonicalized path for a given path string, even if the path
/// or some of its components do not yet exist on the filesystem.
///
/// This function attempts to resolve `.` and `..` components and canonicalize
/// existing parts of the path, but will not fail if a full canonicalization
/// is not possible due to non-existent parts.
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
