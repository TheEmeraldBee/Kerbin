use std::collections::BTreeMap;

use ascii_forge::window::crossterm::cursor::SetCursorStyle;

use crate::*;

/// The main element stored in a buffer that is used to render the buffer to the screen
#[derive(Default)]
pub struct BufferRenderer {
    extmarks: BTreeMap<u64, Extmark>,
    next_id: u64,

    /// The visual representation of the viewport for rendering
    pub lines: Vec<RenderLine>,

    /// Stores a byte position and cursor style for where the renderer should be rendering the cursor
    pub cursor: Option<(usize, SetCursorStyle)>,

    /// The byte based scroll of the window
    pub byte_scroll: usize,

    /// The visual scroll, marks where rendered items should be offset based on the byte_scroll
    pub visual_scroll: usize,

    /// The scroll horizontally of the lines
    pub h_scroll: usize,
}

impl BufferRenderer {
    /// Creates a new extmark in this buffer
    pub fn add_extmark(&mut self, file_version: u128, builder: ExtmarkBuilder) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let ext = builder.build(id, file_version);
        self.extmarks.insert(id, ext);
        id
    }

    /// Adjusts all extmarks based on a single edit operation
    pub fn adjust_extmarks_for_edit(
        &mut self,
        file_version: u128,
        edit_start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) {
        let deleted_len = old_end_byte.saturating_sub(edit_start_byte);
        let inserted_len = new_end_byte.saturating_sub(edit_start_byte);
        let delta = inserted_len as isize - deleted_len as isize;

        let mut to_remove = Vec::new();

        for (id, ext) in self.extmarks.iter_mut() {
            if ext.file_version >= file_version {
                continue;
            }

            match ext.adjustment {
                ExtmarkAdjustment::Fixed => continue,
                ExtmarkAdjustment::DeleteOnDelete => {
                    // Delete if the edit overlaps with the extmark
                    if edit_start_byte < ext.byte_range.end && old_end_byte > ext.byte_range.start {
                        to_remove.push(*id);
                        continue;
                    }
                }
                ExtmarkAdjustment::Track => {}
            }

            let start = ext.byte_range.start;
            let end = ext.byte_range.end;

            if old_end_byte <= start {
                ext.byte_range.start = (start as isize + delta).max(0) as usize;
                ext.byte_range.end = (end as isize + delta).max(0) as usize;
            } else if edit_start_byte >= end {
                // No adjustment needed
            } else if edit_start_byte < start && old_end_byte > start {
                // Deletion affects the extmark's start
                if deleted_len > 0 {
                    let deleted_before_start =
                        (old_end_byte.min(start)).saturating_sub(edit_start_byte);
                    ext.byte_range.start = edit_start_byte + inserted_len;

                    if old_end_byte >= end {
                        // Entire extmark was deleted
                        if ext.adjustment == ExtmarkAdjustment::DeleteOnDelete {
                            to_remove.push(*id);
                            continue;
                        }
                        ext.byte_range.end = ext.byte_range.start;
                    } else {
                        ext.byte_range.end = (end as isize - deleted_before_start as isize
                            + inserted_len as isize)
                            .max(ext.byte_range.start as isize)
                            as usize;
                    }
                } else {
                    ext.byte_range.start = (start as isize + delta).max(0) as usize;
                    ext.byte_range.end = (end as isize + delta).max(0) as usize;
                }
            } else if edit_start_byte == start {
                match ext.gravity {
                    ExtmarkGravity::Right => {
                        // Mark moves with inserted text
                        ext.byte_range.start = (start as isize + delta).max(0) as usize;
                        ext.byte_range.end = (end as isize + delta).max(0) as usize;
                    }
                    ExtmarkGravity::Left => {
                        // Mark stays in place
                        if ext.expand_on_insert && inserted_len > 0 {
                            ext.byte_range.end = (end as isize + delta).max(0) as usize;
                        } else if deleted_len > 0 {
                            let chars_deleted_in_range = deleted_len.min(end - start);
                            ext.byte_range.end = end.saturating_sub(chars_deleted_in_range);
                        }
                    }
                }
            } else if edit_start_byte > start && old_end_byte <= end {
                if ext.expand_on_insert {
                    // Expand the range to include inserted text
                    ext.byte_range.end = (end as isize + delta).max(start as isize) as usize;
                } else if deleted_len > 0 {
                    // Shrink the range
                    ext.byte_range.end = end.saturating_sub(deleted_len);
                    if ext.byte_range.end <= ext.byte_range.start {
                        if ext.adjustment == ExtmarkAdjustment::DeleteOnDelete {
                            to_remove.push(*id);
                            continue;
                        }
                        ext.byte_range.end = ext.byte_range.start;
                    }
                }
            } else if edit_start_byte > start && edit_start_byte < end && old_end_byte >= end {
                if deleted_len > 0 {
                    // Truncate the extmark at the edit start
                    ext.byte_range.end = edit_start_byte + inserted_len;
                } else {
                    ext.byte_range.end = (end as isize + delta).max(start as isize) as usize;
                }
            }
        }

        // Remove marked extmarks
        for id in to_remove {
            self.extmarks.remove(&id);
        }
    }

    /// Process all byte changes from the buffer
    pub fn process_byte_changes(
        &mut self,
        file_version: u128,
        byte_changes: &[[((usize, usize), usize); 3]],
    ) {
        for change in byte_changes {
            let start_byte = change[0].1;
            let old_end_byte = change[1].1;
            let new_end_byte = change[2].1;

            self.adjust_extmarks_for_edit(file_version, start_byte, old_end_byte, new_end_byte);
        }
    }

    /// Clears all extmarks with the given namespace from the system
    pub fn clear_extmark_ns(&mut self, ns: impl AsRef<str>) {
        let ns = ns.as_ref();

        self.extmarks.retain(|_, e| e.namespace != ns);
    }

    /// Removes an extmark by its ID
    pub fn remove_extmark(&mut self, id: u64) -> bool {
        self.extmarks.remove(&id).is_some()
    }

    /// Removes all extmarks in a specific namespace that intersect with the given byte range
    pub fn remove_extmarks_in_range(
        &mut self,
        namespace: impl AsRef<str>,
        range: &std::ops::Range<usize>,
    ) {
        let ns = namespace.as_ref();

        self.extmarks.retain(|_, extmark| {
            if extmark.namespace != ns {
                return true;
            }

            let intersects =
                extmark.byte_range.start < range.end && extmark.byte_range.end > range.start;

            !intersects
        });
    }

    /// Updates an existing extmark's decorations
    pub fn update_extmark(&mut self, id: u64, decorations: Vec<ExtmarkDecoration>) -> bool {
        if let Some(ext) = self.extmarks.get_mut(&id) {
            ext.decorations = decorations;
            true
        } else {
            false
        }
    }

    /// Queries extmarks intersecting a byte range
    pub fn query_extmarks(&self, range: std::ops::Range<usize>) -> Vec<&Extmark> {
        let mut marks = self
            .extmarks
            .values()
            .filter(|ext| ext.byte_range.start < range.end && ext.byte_range.end >= range.start)
            .collect::<Vec<_>>();
        marks.sort_by(|x, y| x.priority.cmp(&y.priority));
        marks
    }
}
