use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use crate::*;

/// The main element stored in a buffer that is used to render the buffer to the screen
#[derive(Default)]
pub struct BufferRenderer {
    /// Primary store: id → Extmark
    extmarks: HashMap<u64, Extmark>,
    /// Spatial index: start_byte → [ids starting at that byte].
    /// Invariant: every id in `extmarks` has exactly one entry here.
    start_index: BTreeMap<usize, Vec<u64>>,
    next_id: u64,

    /// Priority per namespace. Marks are rendered in ascending priority order (higher = on top).
    /// Namespaces not in this map default to priority 0.
    namespace_priorities: HashMap<String, i32>,

    /// The byte based scroll of the window
    pub byte_scroll: usize,

    /// The visual scroll, marks where rendered items should be offset based on the byte_scroll
    pub visual_scroll: usize,

    /// The scroll horizontally of the lines
    pub h_scroll: usize,

    /// Set by `scroll_lines` to tell the update loop to clamp the cursor into the viewport
    /// (rather than scrolling to follow the cursor).
    pub cursor_drag: bool,
}


impl BufferRenderer {
    fn index_insert(&mut self, id: u64, start: usize) {
        self.start_index.entry(start).or_default().push(id);
    }

    fn index_remove(&mut self, id: u64, start: usize) {
        if let Some(ids) = self.start_index.get_mut(&start) {
            ids.retain(|&x| x != id);
            if ids.is_empty() {
                self.start_index.remove(&start);
            }
        }
    }

    fn index_move(&mut self, id: u64, old_start: usize, new_start: usize) {
        if old_start != new_start {
            self.index_remove(id, old_start);
            self.index_insert(id, new_start);
        }
    }
}


impl BufferRenderer {
    /// Creates a new extmark in this buffer
    pub fn add_extmark(&mut self, file_version: u128, builder: ExtmarkBuilder) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let ext = builder.build(id, file_version);
        self.index_insert(id, ext.byte_range.start);
        self.extmarks.insert(id, ext);
        id
    }

    /// Atomically replaces all extmarks in a namespace.
    /// More efficient than `clear_extmark_ns` + N × `add_extmark`.
    pub fn set_namespace(
        &mut self,
        file_version: u128,
        ns: &str,
        marks: Vec<ExtmarkBuilder>,
    ) -> Vec<u64> {
        // Remove existing marks in this namespace
        let old_ids: Vec<u64> = self
            .extmarks
            .values()
            .filter(|e| e.namespace == ns)
            .map(|e| e.id)
            .collect();
        for id in old_ids {
            let start = self.extmarks[&id].byte_range.start;
            self.index_remove(id, start);
            self.extmarks.remove(&id);
        }

        // Insert new marks
        marks
            .into_iter()
            .map(|builder| {
                let id = self.next_id;
                self.next_id += 1;
                let ext = builder.build(id, file_version);
                self.index_insert(id, ext.byte_range.start);
                self.extmarks.insert(id, ext);
                id
            })
            .collect()
    }

    /// Clears all extmarks with the given namespace
    pub fn clear_extmark_ns(&mut self, ns: impl AsRef<str>) {
        let ns = ns.as_ref();
        let ids: Vec<u64> = self
            .extmarks
            .values()
            .filter(|e| e.namespace == ns)
            .map(|e| e.id)
            .collect();
        for id in ids {
            let start = self.extmarks[&id].byte_range.start;
            self.index_remove(id, start);
            self.extmarks.remove(&id);
        }
    }

    /// Removes an extmark by its ID
    pub fn remove_extmark(&mut self, id: u64) -> bool {
        if let Some(ext) = self.extmarks.remove(&id) {
            self.index_remove(id, ext.byte_range.start);
            true
        } else {
            false
        }
    }

    /// Removes all extmarks in a namespace that intersect with the given byte range
    pub fn remove_extmarks_in_range(&mut self, namespace: impl AsRef<str>, range: &Range<usize>) {
        let ns = namespace.as_ref();
        let ids: Vec<u64> = self
            .extmarks
            .values()
            .filter(|e| {
                e.namespace == ns
                    && e.byte_range.start < range.end
                    && e.byte_range.end > range.start
            })
            .map(|e| e.id)
            .collect();
        for id in ids {
            let start = self.extmarks[&id].byte_range.start;
            self.index_remove(id, start);
            self.extmarks.remove(&id);
        }
    }

    /// Sets the rendering priority for all extmarks in a namespace.
    /// Higher values render on top of lower values. Defaults to 0 if not set.
    pub fn set_namespace_priority(&mut self, ns: impl Into<String>, priority: i32) {
        self.namespace_priorities.insert(ns.into(), priority);
    }

    /// Returns the registered priority for a namespace, or 0 if unregistered.
    pub fn ns_priority(&self, ns: &str) -> i32 {
        self.namespace_priorities.get(ns).copied().unwrap_or(0)
    }

    /// Updates an existing extmark's kind and/or byte range.
    pub fn update_extmark(&mut self, id: u64, kind: Option<ExtmarkKind>, byte_range: Option<Range<usize>>) -> bool {
        let Some(ext) = self.extmarks.get_mut(&id) else {
            return false;
        };
        if let Some(k) = kind {
            ext.kind = k;
        }
        if let Some(new_range) = byte_range {
            let old_start = ext.byte_range.start;
            let new_start = new_range.start;
            ext.byte_range = new_range;
            self.index_move(id, old_start, new_start);
        }
        true
    }

    /// Queries extmarks intersecting a byte range, sorted by namespace priority (ascending).
    ///
    /// Uses the spatial start-index to scan only marks whose start byte is less than
    /// `range.end`, giving O(k log n) performance where k is the number of matching marks.
    pub fn query_extmarks(&self, range: Range<usize>) -> Vec<&Extmark> {
        let mut marks: Vec<&Extmark> = self
            .start_index
            .range(..range.end)
            .flat_map(|(_, ids)| ids.iter())
            .filter_map(|id| self.extmarks.get(id))
            .filter(|ext| ext.byte_range.end >= range.start)
            .collect();
        marks.sort_by_key(|x| self.ns_priority(&x.namespace));
        marks
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

    /// Adjusts all extmarks to account for a single edit operation.
    pub fn adjust_extmarks_for_edit(
        &mut self,
        file_version: u128,
        edit_start: usize,
        old_end: usize,
        new_end: usize,
    ) {
        let ids: Vec<u64> = self.extmarks.keys().copied().collect();
        let mut to_remove: Vec<u64> = Vec::new();
        let mut to_reindex: Vec<(u64, usize, usize)> = Vec::new();

        for id in ids {
            let ext = &self.extmarks[&id];

            // Skip marks created after this edit — they don't need adjustment.
            // Use > (not >=) so marks stamped at the same version as this edit are adjusted.
            if ext.file_version > file_version {
                continue;
            }

            match adjust_mark_range(ext, edit_start, old_end, new_end) {
                None => to_remove.push(id),
                Some(new_range) => {
                    let old_start = ext.byte_range.start;
                    let new_start = new_range.start;
                    self.extmarks.get_mut(&id).unwrap().byte_range = new_range;
                    if old_start != new_start {
                        to_reindex.push((id, old_start, new_start));
                    }
                }
            }
        }

        for id in to_remove {
            let start = self.extmarks[&id].byte_range.start;
            self.index_remove(id, start);
            self.extmarks.remove(&id);
        }
        for (id, old_start, new_start) in to_reindex {
            self.index_move(id, old_start, new_start);
        }
    }
}


/// Compute the new byte range for a mark after one edit operation.
/// Returns `None` if the mark should be deleted.
///
/// The edit replaces bytes `[edit_start, old_end)` with `new_end - edit_start` new bytes.
/// Six disjoint positional relationships are handled:
///
/// ```text
/// A: edit entirely after mark   — no change
/// B: edit entirely before mark  — shift both endpoints by delta
/// C: edit overlaps mark start   — clamp/adjust start
/// D: edit at or inside mark     — adjust end (gravity matters when edit_start == mark_start)
/// E: edit overlaps mark end     — clamp end
/// F: edit contains mark         — collapse to zero-width (or delete if DeleteOnDelete)
/// ```
fn adjust_mark_range(
    mark: &Extmark,
    edit_start: usize,
    old_end: usize,
    new_end: usize,
) -> Option<Range<usize>> {
    let deleted = old_end.saturating_sub(edit_start);
    let inserted = new_end.saturating_sub(edit_start);
    let delta: isize = inserted as isize - deleted as isize;

    let ms = mark.byte_range.start;
    let me = mark.byte_range.end;

    // A: edit is entirely after the mark
    if edit_start >= me {
        return Some(ms..me);
    }

    // Fixed marks never move, regardless of edit position.
    if mark.adjustment == ExtmarkAdjustment::Fixed {
        return Some(ms..me);
    }

    // B: edit is entirely before the mark.
    // For pure insertions (deleted == 0) exactly at mark start, fall through to gravity handling.
    if old_end < ms || (old_end == ms && deleted > 0) {
        return Some(shift(ms, delta)..shift(me, delta));
    }

    // Pure insertion at mark start: gravity determines whether the mark moves.
    if edit_start == ms && deleted == 0 {
        return Some(adjust_at_start(mark, delta, inserted, deleted, me));
    }

    // F: edit entirely contains mark — collapse or delete
    if edit_start <= ms && old_end >= me {
        if mark.adjustment == ExtmarkAdjustment::DeleteOnDelete {
            return None;
        }
        let point = edit_start + inserted;
        return Some(point..point);
    }

    // C: edit overlaps mark start but not end
    if edit_start < ms && old_end < me {
        if mark.adjustment == ExtmarkAdjustment::DeleteOnDelete {
            return None;
        }
        let new_start = edit_start + inserted;
        let deleted_before_start = old_end.saturating_sub(ms);
        let new_end_val = (me as isize - deleted_before_start as isize)
            .max(new_start as isize) as usize;
        return Some(new_start..new_end_val);
    }

    // E: edit overlaps mark end but not start
    if edit_start > ms && old_end >= me {
        if deleted > 0 {
            let new_end_val = (edit_start + inserted).max(ms);
            return Some(ms..new_end_val);
        } else {
            return Some(ms..shift(me, delta));
        }
    }

    // D: edit is strictly inside or at the start of the mark
    if edit_start == ms {
        return Some(adjust_at_start(mark, delta, inserted, deleted, me));
    }

    // edit_start > ms && old_end < me: edit strictly inside the mark
    if mark.expand_on_insert || deleted == 0 {
        Some(ms..shift(me, delta).max(ms))
    } else {
        let new_end_val = me.saturating_sub(deleted).max(ms);
        Some(ms..new_end_val)
    }
}

/// Handle the case where `edit_start == mark_start` (gravity applies).
fn adjust_at_start(
    mark: &Extmark,
    delta: isize,
    inserted: usize,
    deleted: usize,
    me: usize,
) -> Range<usize> {
    let ms = mark.byte_range.start;
    match mark.gravity {
        // Right gravity: mark moves with inserted text
        ExtmarkGravity::Right => shift(ms, delta)..shift(me, delta),
        ExtmarkGravity::Left => {
            if mark.expand_on_insert && inserted > 0 {
                // Expand: keep start, extend end
                ms..shift(me, delta).max(ms)
            } else if deleted > 0 {
                let deleted_in_range = deleted.min(me.saturating_sub(ms));
                ms..me.saturating_sub(deleted_in_range)
            } else {
                ms..me
            }
        }
    }
}

fn shift(pos: usize, delta: isize) -> usize {
    (pos as isize + delta).max(0) as usize
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExtmarkAdjustment, ExtmarkGravity, ExtmarkKind};
    use ratatui::style::Style;

    fn mark(start: usize, end: usize) -> Extmark {
        Extmark {
            id: 0,
            file_version: 0,
            namespace: "test".into(),
            byte_range: start..end,
            kind: ExtmarkKind::Highlight { style: Style::default() },
            gravity: ExtmarkGravity::Right,
            adjustment: ExtmarkAdjustment::Track,
            expand_on_insert: false,
        }
    }

    fn mark_with(start: usize, end: usize, gravity: ExtmarkGravity, expand: bool, adj: ExtmarkAdjustment) -> Extmark {
        Extmark { gravity, expand_on_insert: expand, adjustment: adj, ..mark(start, end) }
    }

    fn adj(m: &Extmark, es: usize, oe: usize, ne: usize) -> Option<Range<usize>> {
        adjust_mark_range(m, es, oe, ne)
    }

    // Case A: edit after mark
    #[test]
    fn edit_after_mark() {
        assert_eq!(adj(&mark(2, 5), 6, 8, 10), Some(2..5));
    }

    // Case A: edit starting exactly at mark end
    #[test]
    fn edit_at_mark_end() {
        assert_eq!(adj(&mark(2, 5), 5, 7, 9), Some(2..5));
    }

    // Case B: pure insert before mark
    #[test]
    fn insert_before_mark() {
        assert_eq!(adj(&mark(5, 10), 2, 2, 5), Some(8..13));
    }

    // Case B: delete before mark
    #[test]
    fn delete_before_mark() {
        assert_eq!(adj(&mark(5, 10), 2, 4, 2), Some(3..8));
    }

    // Case F: edit contains mark — collapse
    #[test]
    fn edit_contains_mark_collapses() {
        // edit [1,11) contains mark [3,7)
        assert_eq!(adj(&mark(3, 7), 1, 11, 5), Some(5..5));
    }

    // Case F: edit contains mark — delete-on-delete
    #[test]
    fn edit_contains_mark_deletes() {
        let m = mark_with(3, 7, ExtmarkGravity::Right, false, ExtmarkAdjustment::DeleteOnDelete);
        assert_eq!(adj(&m, 1, 11, 5), None);
    }

    // Case C: edit overlaps mark start
    #[test]
    fn edit_overlaps_start() {
        // edit [3,7) on mark [5,10) → new start = 3+2=5, deleted_before_start=7-5=2, new_end=10-2=8
        assert_eq!(adj(&mark(5, 10), 3, 7, 5), Some(5..8));
    }

    // Case E: edit overlaps mark end
    #[test]
    fn edit_overlaps_end() {
        // edit [7,12) on mark [5,10) → new end = 7+0=7 (delete), mark becomes [5,7)
        assert_eq!(adj(&mark(5, 10), 7, 12, 7), Some(5..7));
    }

    // Case D: insert strictly inside mark
    #[test]
    fn insert_inside_mark_no_expand() {
        // edit [6,6) insert 3 bytes inside mark [4,10) → end shifts by 3
        assert_eq!(adj(&mark(4, 10), 6, 6, 9), Some(4..13));
    }

    // Case D: insert at mark start, right gravity
    #[test]
    fn insert_at_start_right_gravity() {
        let m = mark_with(5, 10, ExtmarkGravity::Right, false, ExtmarkAdjustment::Track);
        // insert 3 bytes at 5 → mark shifts right
        assert_eq!(adj(&m, 5, 5, 8), Some(8..13));
    }

    // Case D: insert at mark start, left gravity
    #[test]
    fn insert_at_start_left_gravity() {
        let m = mark_with(5, 10, ExtmarkGravity::Left, false, ExtmarkAdjustment::Track);
        // insert 3 bytes at 5 → mark stays
        assert_eq!(adj(&m, 5, 5, 8), Some(5..10));
    }

    // Case D: insert at mark start, left gravity with expand
    #[test]
    fn insert_at_start_left_gravity_expand() {
        let m = mark_with(5, 10, ExtmarkGravity::Left, true, ExtmarkAdjustment::Track);
        // insert 3 bytes at 5 → start stays, end expands
        assert_eq!(adj(&m, 5, 5, 8), Some(5..13));
    }

    // Fixed adjustment: never moves
    #[test]
    fn fixed_adjustment() {
        let m = mark_with(5, 10, ExtmarkGravity::Right, false, ExtmarkAdjustment::Fixed);
        assert_eq!(adj(&m, 3, 3, 6), Some(5..10));
        assert_eq!(adj(&m, 3, 6, 3), Some(5..10));
    }
}
