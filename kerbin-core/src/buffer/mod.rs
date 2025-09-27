use std::{
    collections::BTreeMap,
    io::{BufReader, BufWriter, ErrorKind, Write},
    ops::Range,
    path::{Path, PathBuf},
};

pub mod action;
pub use action::*;

pub mod buffers;
pub use buffers::*;

pub mod systems;
pub use systems::*;

pub mod cursor;
pub use cursor::*;

pub mod extmark;
pub use extmark::*;

use ropey::{LineType, Rope};

/// Used internally for defining a set of actions that were applied together as a single undo/redo unit.
///
/// A `ChangeGroup` stores the state of cursors *before* the actions were applied,
/// and a list of `BufferAction` inverses to reverse the changes.
#[derive(Default)]
pub struct ChangeGroup(Vec<Cursor>, Vec<Box<dyn BufferAction>>);

/// The core storage of an open text buffer inside of the editor.
///
/// `TextBuffer` is responsible for storing file content (`ropey::Rope`),
/// managing file metadata (path, extension), tracking multiple cursors,
/// handling undo/redo, and managing scroll positions for rendering.
pub struct TextBuffer {
    /// Internal storage of the text itself using `ropey::Rope`.
    ///
    /// Changes to the `Rope` should primarily be made through `BufferAction`s
    /// to correctly integrate with undo/redo and syntax highlighting systems.
    pub rope: Rope,

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

    /// A list of data that marks byte changes applied to the rope.
    /// Each entry is an array of three `((row, col), byte_idx)` tuples:
    /// `[0]` is the start position of the edit.
    /// `[1]` is the previous ending position of the edit.
    /// `[2]` is the new ending position of the edit.
    /// This is used for systems like incremental syntax highlighting updates.
    pub byte_changes: Vec<[((usize, usize), usize); 3]>,

    /// An optional `ChangeGroup` currently being built.
    /// Actions are added to this group until `commit_change_group` is called.
    current_change: Option<ChangeGroup>,

    /// A stack of `ChangeGroup`s representing past changes that can be undone.
    undo_stack: Vec<ChangeGroup>,
    /// A stack of `ChangeGroup`s representing undone changes that can be redone.
    redo_stack: Vec<ChangeGroup>,

    /// The current vertical scroll offset (line index) of the buffer's viewport.
    /// Used for rendering only a visible portion of the file.
    pub scroll: usize,

    /// The current horizontal scroll offset (character index) of the buffer's viewport.
    /// Used for horizontal scrolling within long lines.
    pub h_scroll: usize,

    /// Table of extmarks currently attached to this buffer.
    pub extmarks: BTreeMap<u64, Extmark>,

    /// Auto-incrementing counter for assigning extmark IDs.
    pub next_extmark_id: u64,
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
        Self {
            rope: Rope::new(),

            path: "<scratch>".into(),
            ext: "".into(),

            cursors: vec![Cursor::default()],
            primary_cursor: 0,

            byte_changes: vec![],

            current_change: None,

            undo_stack: vec![],
            redo_stack: vec![],

            scroll: 0,
            h_scroll: 0,

            extmarks: BTreeMap::new(),
            next_extmark_id: 0,
        }
    }

    /// Opens a file with the provided path, loading its content into the buffer.
    ///
    /// The `path_str` can be absolute or relative. It will be canonicalized,
    /// even if the file does not yet exist. This method also handles
    /// extracting the file extension, initializing cursors, and reading file content.
    /// If the file does not exist, an empty buffer is created.
    ///
    /// # Arguments
    ///
    /// * `path_str`: The string path to the file to open.
    ///
    /// # Returns
    ///
    /// A new `TextBuffer` instance with the file's content and metadata.
    pub fn open(path_str: String) -> Self {
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

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            found_ext = ext.to_string();
        }

        Self {
            rope,
            path: path.to_str().map(|x| x.to_string()).unwrap_or_default(),
            ext: found_ext,

            cursors: vec![Cursor::default()],
            primary_cursor: 0,

            byte_changes: vec![],

            undo_stack: vec![],
            redo_stack: vec![],
            current_change: None,

            scroll: 0,
            h_scroll: 0,

            extmarks: BTreeMap::new(),
            next_extmark_id: 0,
        }
    }

    /// Creates a new extmark in this buffer, with a single byte pos
    ///
    /// # Arguments
    /// * `ns` - The Namespace of the Extmark
    /// * `byte` - The byte index to place the extmark.
    /// * `priority` - Rendering priority (higher → drawn on top).
    /// * `decorations` - A vector of [`ExtmarkDecoration`] items.
    ///
    /// # Returns
    /// The unique ID of the newly created extmark.
    pub fn add_extmark(
        &mut self,
        ns: impl ToString,
        byte: usize,
        priority: i32,
        decorations: Vec<ExtmarkDecoration>,
    ) -> u64 {
        let id = self.next_extmark_id;
        self.next_extmark_id += 1;
        let ext = Extmark {
            namespace: ns.to_string(),
            id,
            byte_range: byte..byte + 1,
            priority,
            decorations,
        };
        self.extmarks.insert(id, ext);
        id
    }

    /// Creates a new extmark in this buffer, taking up the given range of bytes
    ///
    /// # Arguments
    /// * `ns` - The Namespace of the Extmark
    /// * `byte_range` - The range of bytes that the decorations take up.
    /// * `priority` - Rendering priority (higher → drawn on top).
    /// * `decorations` - A vector of [`ExtmarkDecoration`] items.
    ///
    /// # Returns
    /// The unique ID of the newly created extmark.
    pub fn add_extmark_range(
        &mut self,
        ns: impl ToString,
        byte_range: Range<usize>,
        priority: i32,
        decorations: Vec<ExtmarkDecoration>,
    ) -> u64 {
        let id = self.next_extmark_id;
        self.next_extmark_id += 1;
        let ext = Extmark {
            namespace: ns.to_string(),
            id,
            byte_range,
            priority,
            decorations,
        };
        self.extmarks.insert(id, ext);
        id
    }

    /// Clears all extmarks with the given namespace from the system
    ///
    /// # Arguments
    /// * 'ns' - The namespace to remove
    pub fn clear_extmark_ns(&mut self, ns: impl AsRef<str>) {
        let ns = ns.as_ref();

        self.extmarks.retain(|_, e| &e.namespace != ns);
    }

    /// Removes an extmark by its ID.
    ///
    /// # Arguments
    /// * `id` - The id of the extmark to remove
    ///
    /// # Returns
    /// `true` if successfully removed, `false` otherwise.
    pub fn remove_extmark(&mut self, id: u64) -> bool {
        self.extmarks.remove(&id).is_some()
    }

    /// Updates an existing extmark's decorations.
    ///
    /// # Arguments
    /// * `id` - The id of the extmark to update
    /// * `decorations` - A list of decorations to set the ID to
    ///
    /// # Returns
    /// `true` if the extmark exists and was updated, `false` otherwise.
    pub fn update_extmark(&mut self, id: u64, decorations: Vec<ExtmarkDecoration>) -> bool {
        if let Some(ext) = self.extmarks.get_mut(&id) {
            ext.decorations = decorations;
            true
        } else {
            false
        }
    }

    /// Queries extmarks intersecting a byte range.
    ///
    /// # Arguments
    /// * `range` - The byte range that should be included in the extmark list
    ///
    /// # Returns
    ///
    /// A list of extmarks found within the given range
    pub fn query_extmarks(&self, range: std::ops::Range<usize>) -> Vec<&Extmark> {
        let mut marks = self
            .extmarks
            .values()
            .filter(|ext| ext.byte_range.start < range.end && ext.byte_range.end >= range.start)
            .collect::<Vec<_>>();
        marks.sort_by(|x, y| x.priority.cmp(&y.priority));
        marks
    }

    /// Given a byte offset, returns a tuple containing the `((line_idx, col_idx), byte_idx)`.
    ///
    /// This format is convenient for registering edits within the `byte_changes` vector,
    /// particularly for external tools like Tree-sitter.
    ///
    /// # Arguments
    ///
    /// * `byte`: The byte offset within the rope.
    ///
    /// # Returns
    ///
    /// A tuple `((usize, usize), usize)` representing `((line_index, column_index), byte_index)`.
    pub fn get_edit_part(&self, byte: usize) -> ((usize, usize), usize) {
        let line_idx = self.rope.byte_to_line_idx(byte, LineType::LF_CR);
        let col = byte - self.rope.line_to_byte_idx(line_idx, LineType::LF_CR);

        ((line_idx, col), byte)
    }

    /// Registers an edit with the buffer for tracking changes.
    ///
    /// This method stores the start, old end, and new end positions of an edit.
    /// This information is crucial for systems that need to react to buffer changes,
    /// such as syntax highlighting or language server protocols.
    ///
    /// # Arguments
    ///
    /// * `start`: The `((line, col), byte)` tuple for the start of the edit.
    /// * `old_end`: The `((line, col), byte)` tuple for the end of the previous state of the edit.
    /// * `new_end`: The `((line, col), byte)` tuple for the end of the new state of the edit.
    pub fn register_input_edit(
        &mut self,
        start: ((usize, usize), usize),
        old_end: ((usize, usize), usize),
        new_end: ((usize, usize), usize),
    ) {
        self.byte_changes.push([start, old_end, new_end]);
    }

    /// Applies a given `BufferAction` to the editor.
    ///
    /// This is the primary method for making all modifications to the buffer's content.
    /// It automatically handles:
    /// - Grouping actions for undo/redo.
    /// - Clearing the redo stack upon new changes.
    /// - Returning a boolean indicating the success of the action.
    ///
    /// # Arguments
    ///
    /// * `action`: An instance of a type implementing `BufferAction` to be applied.
    ///
    /// # Returns
    ///
    /// `true` if the action was successfully applied, `false` otherwise.
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
    ///
    /// # Arguments
    ///
    /// * `offset`: The signed offset to move the primary cursor index (e.g., `1` for next, `-1` for previous).
    pub fn change_cursor(&mut self, offset: isize) {
        self.primary_cursor = self
            .primary_cursor
            .saturating_add_signed(offset)
            .clamp(0, self.cursors.len() - 1);
    }

    /// Returns an immutable reference to the current primary cursor of the buffer.
    ///
    /// # Returns
    ///
    /// A `&Cursor` representing the primary cursor.
    pub fn primary_cursor(&self) -> &Cursor {
        &self.cursors[self.primary_cursor]
    }

    /// Returns a mutable reference to the current primary cursor of the buffer.
    ///
    /// # Returns
    ///
    /// A `&mut Cursor` representing the primary cursor.
    pub fn primary_cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursors[self.primary_cursor]
    }

    /// Applies the undo operation of the last `ChangeGroup` on the undo stack.
    ///
    /// This effectively reverts the most recent group of changes. It also records
    /// the inverse of these inversed actions onto the redo stack.
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

    /// Applies the redo operation from the redo stack.
    ///
    /// This re-applies a previously undone `ChangeGroup`. It also records
    /// the inverse of these redone actions onto the undo stack.
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

    /// Scrolls the buffer vertically by a given number of lines.
    ///
    /// The scroll position is clamped to prevent scrolling past the start
    /// or end of the document.
    ///
    /// # Arguments
    ///
    /// * `delta`: The signed number of lines to scroll (positive for down, negative for up).
    ///
    /// # Returns
    ///
    /// `true` if the scroll position changed, `false` otherwise.
    pub fn scroll_lines(&mut self, delta: isize) -> bool {
        if delta == 0 {
            return true;
        }

        let old_scroll = self.scroll;
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .clamp(0, self.rope.len_lines(LineType::LF_CR).saturating_sub(1));

        self.scroll != old_scroll
    }

    /// Scrolls the buffer horizontally by a given number of characters.
    ///
    /// The scroll position is clamped to prevent scrolling past the start of a line.
    /// There is no explicit right clamp as lines can be arbitrarily long.
    ///
    /// # Arguments
    ///
    /// * `delta`: The signed number of characters to scroll (positive for right, negative for left).
    ///
    /// # Returns
    ///
    /// `true` if the scroll position changed, `false` otherwise.
    pub fn scroll_horizontal(&mut self, delta: isize) -> bool {
        if delta == 0 {
            return true;
        }
        let old_scroll = self.h_scroll;
        self.h_scroll = self.h_scroll.saturating_add_signed(delta).max(0);
        self.h_scroll != old_scroll
    }

    /// Writes the buffer's content to a file on disk.
    ///
    /// If `path` is `Some(String)`, the buffer's internal `path` is updated,
    /// and the file is written to the new path. If `path` is `None`, the file
    /// is written to the buffer's currently stored `path`.
    ///
    /// Handles directory creation and ensures the file exists.
    /// A scratch file (`<scratch>`) cannot be written without providing a new path.
    ///
    /// # Arguments
    ///
    /// * `path`: An `Option<String>` representing a new path to save to, or `None` to save to the current path.
    ///
    /// # Panics
    ///
    /// Panics if unable to create directories, create the file, or if writing to disk fails unexpectedly.
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
            )
            .unwrap();
    }

    /// Moves the primary cursor by a given number of bytes.
    ///
    /// This function directly adjusts the cursor's position by `bytes` within the buffer's
    /// content, clamping the new position to be within the valid range of the `Rope`.
    /// It can optionally extend the current selection.
    ///
    /// # Arguments
    ///
    /// * `bytes`: The signed number of bytes to move the cursor (positive for forward, negative for backward).
    /// * `extend_selection`: If `true`, the selection will be extended. If `false`,
    ///   the selection will collapse to the new cursor position.
    ///
    /// # Returns
    ///
    /// `true` if the cursor successfully moved to a new byte position, `false` otherwise.
    pub fn move_bytes(&mut self, bytes: isize, extend_selection: bool) -> bool {
        if bytes == 0 {
            return false;
        }

        let current_cursor = self.primary_cursor();
        let current_caret_byte = current_cursor.get_cursor_byte();

        let new_caret_byte =
            (current_caret_byte as isize + bytes).clamp(0, self.rope.len() as isize) as usize;

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
    ///
    /// # Arguments
    ///
    /// * `rows`: The signed number of rows to move the cursor (positive for down, negative for up).
    /// * `extend_selection`: If `true`, the selection will be extended. If `false`,
    ///   the selection will collapse to the new cursor position.
    ///
    /// # Returns
    ///
    /// `true` if the cursor successfully moved to a new byte position, `false` otherwise.
    pub fn move_lines(&mut self, rows: isize, extend_selection: bool) -> bool {
        if rows == 0 {
            return false;
        }

        let current_cursor = self.primary_cursor();
        let current_caret_byte = current_cursor.get_cursor_byte();

        let current_line_idx = self
            .rope
            .byte_to_line_idx(current_caret_byte, LineType::LF_CR);
        let line_start_byte = self
            .rope
            .line_to_byte_idx(current_line_idx, LineType::LF_CR);
        let current_col_char_idx = self.rope.byte_to_char_idx(current_caret_byte)
            - self.rope.byte_to_char_idx(line_start_byte);

        let total_lines = self.rope.len_lines(LineType::LF_CR);
        let mut target_line_idx = current_line_idx.saturating_add_signed(rows);
        target_line_idx = target_line_idx.clamp(0, total_lines.saturating_sub(1));

        let line_slice = self.rope.line(target_line_idx, LineType::LF_CR);
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

        let new_caret_byte = self.rope.line_to_byte_idx(target_line_idx, LineType::LF_CR)
            + self
                .rope
                .line(target_line_idx, LineType::LF_CR)
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
    ///
    /// # Arguments
    ///
    /// * `chars`: The signed number of characters to move the cursor (positive for forward, negative for backward).
    /// * `extend_selection`: If `true`, the selection will be extended. If `false`,
    ///   the selection will collapse to the new cursor position.
    ///
    /// # Returns
    ///
    /// `true` if the cursor successfully moved to a new character position, `false` otherwise.
    pub fn move_chars(&mut self, chars: isize, extend_selection: bool) -> bool {
        if chars == 0 {
            return false;
        }

        let current_cursor = self.primary_cursor().clone();
        let current_caret_char_idx = self.rope.byte_to_char_idx(current_cursor.get_cursor_byte());

        let new_caret_char_idx = (current_caret_char_idx as isize + chars)
            .clamp(0, self.rope.len_chars() as isize) as usize;

        let new_caret_byte = self.rope.char_to_byte_idx(new_caret_char_idx);

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

    /// Applies the major update functions all in one convenient function.
    ///
    /// This method is typically called once per frame and performs necessary
    /// housekeeping tasks, such as merging overlapping cursors and clearing
    /// the `byte_changes` list for the next frame.
    pub fn update(&mut self) {
        self.merge_overlapping_cursors();
        self.byte_changes.clear();
    }
}

/// Computes the canonicalized path for a given path string, even if the path
/// or some of its components do not yet exist on the filesystem.
///
/// This function attempts to resolve `.` and `..` components and canonicalize
/// existing parts of the path, but will not fail if a full canonicalization
/// is not possible due to non-existent parts.
///
/// # Arguments
///
/// * `path_str`: A string slice representing the path to canonicalize.
///
/// # Returns
///
/// A `PathBuf` representing the best-effort canonicalized path.
///
/// # Panics
///
/// Panics if `std::env::current_dir()` fails when resolving a relative path.
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
