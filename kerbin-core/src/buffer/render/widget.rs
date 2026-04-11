use std::sync::Arc;

use crate::{
    ConcealScope, CursorShape, Extmark, ExtmarkKind, OverlayPosition, OverlayWidget, StyledChunk,
    TextBuffer, VirtTextPos, grapheme_display_width,
};
use ratatui::prelude::*;
use ropey::{Rope, RopeSlice};
use unicode_segmentation::UnicodeSegmentation;

/// A widget to render from a text buffer onto the screen
pub struct TextBufferWidget<'a> {
    buf: &'a TextBuffer,
    line_scroll: usize,
    h_scroll: usize,
    tab_display_unit: String,
    tab_style: Style,
    reveal_conceal_on_cursor_line: bool,
}

impl<'a> TextBufferWidget<'a> {
    pub fn new(buf: &'a TextBuffer) -> Self {
        Self {
            buf,
            line_scroll: 0,
            h_scroll: 0,
            tab_display_unit: "    ".to_string(),
            tab_style: Style::default(),
            reveal_conceal_on_cursor_line: true,
        }
    }

    pub fn with_reveal_conceal_on_cursor_line(mut self, reveal: bool) -> Self {
        self.reveal_conceal_on_cursor_line = reveal;
        self
    }

    pub fn with_vertical_scroll(mut self, lines: usize) -> Self {
        self.line_scroll = lines;
        self
    }

    pub fn with_horizontal_scroll(mut self, scroll: usize) -> Self {
        self.h_scroll = scroll;
        self
    }

    pub fn with_tab_display_unit(mut self, unit: String) -> Self {
        self.tab_display_unit = unit;
        self
    }

    pub fn with_tab_style(mut self, style: Style) -> Self {
        self.tab_style = style;
        self
    }

    #[allow(clippy::too_many_arguments)]
    fn render_marked_line(
        &self,
        rope: &Rope,
        marks: &[&Extmark],
        rope_line: RopeSlice<'_>,
        line_start_char: usize,
        line_end_char: usize,
        line_char_count: usize,
        visible_len: usize,
        extra_eof_space: bool,
        width: usize,
        reveal_conceal_on_cursor_line: bool,
    ) -> LineRenderResult {
        let char_marks: Vec<Extmark> = marks
            .iter()
            .map(|mark| {
                let start_char = rope.byte_to_char(mark.byte_range.start.min(rope.len_bytes()));
                let end_char = rope.byte_to_char(mark.byte_range.end.min(rope.len_bytes()))
                    + usize::from(extra_eof_space && mark.byte_range.end > rope.len_bytes());
                Extmark {
                    file_version: mark.file_version,
                    id: mark.id,
                    namespace: mark.namespace.clone(),
                    byte_range: start_char..end_char,
                    kind: mark.kind.clone(),
                    gravity: mark.gravity,
                    adjustment: mark.adjustment,
                    expand_on_insert: mark.expand_on_insert,
                }
            })
            .collect();

        let char_marks_refs: Vec<&Extmark> = char_marks.iter().collect();

        let ns_priority = |ns: &str| self.buf.renderer.ns_priority(ns);
        let eof_extra = extra_eof_space as usize;
        let mut lm = LineMarks::classify(
            &char_marks_refs,
            line_start_char,
            line_end_char + eof_extra,
            line_char_count + eof_extra,
            visible_len + eof_extra,
            reveal_conceal_on_cursor_line,
            &ns_priority,
        );

        let full_line_text: String = {
            let base = rope_line
                .to_string()
                .chars()
                .take(visible_len)
                .collect::<String>();
            if extra_eof_space { base + " " } else { base }
        };

        // Apply whitespace trimming to conceals, producing EffectiveConceal for downstream use.
        let chars: Vec<char> = full_line_text.chars().collect();
        let total_chars = chars.len();
        let effective_conceals: Vec<EffectiveConceal> = {
            let mut result = Vec::with_capacity(lm.conceals.len());
            let mut prev_end = 0usize;
            for (i, cm) in lm.conceals.iter().enumerate() {
                let actual_start = if cm.trim_before {
                    let mut s = cm.start;
                    while s > prev_end
                        && chars
                            .get(s.wrapping_sub(1))
                            .map(|ch: &char| ch.is_ascii_whitespace())
                            .unwrap_or(false)
                    {
                        s -= 1;
                    }
                    s
                } else {
                    cm.start
                };
                let next_start = lm.conceals.get(i + 1).map(|c| c.start).unwrap_or(total_chars);
                let actual_end = if cm.trim_after {
                    let mut e = cm.end;
                    while e < next_start
                        && chars
                            .get(e)
                            .map(|ch: &char| ch.is_ascii_whitespace())
                            .unwrap_or(false)
                    {
                        e += 1;
                    }
                    e
                } else {
                    cm.end
                };
                result.push(EffectiveConceal {
                    start: actual_start,
                    end: actual_end,
                    replacement: cm.replacement,
                    style: cm.style,
                });
                prev_end = actual_end;
            }
            result
        };

        let (seg_list, col_ranges) = build_concealed_segments(&full_line_text, &effective_conceals);

        let mut segments = apply_highlights(
            seg_list,
            &col_ranges,
            &mut lm.highlights,
            &effective_conceals,
            &lm.newline_highlights,
            visible_len + eof_extra,
        );

        if !lm.eol_highlights.is_empty() {
            lm.eol_highlights.sort_by_key(|(_, p)| *p);
            let mut eol_style = Style::default();
            for (s, _) in &lm.eol_highlights {
                eol_style = eol_style.patch(*s);
            }
            segments.push(StyledSegment {
                text: " ".to_string(),
                style: eol_style,
            });
        }

        lm.inline_marks.sort_by_key(|m| m.col);
        for m in lm.inline_marks.iter().rev() {
            let display_col = buffer_to_display(m.col, &effective_conceals);
            segments.insert_at(display_col, m.chunks);
        }

        for m in &lm.overlay_marks {
            let display_col = buffer_to_display(m.col, &effective_conceals);
            segments.overlay_at(display_col, m.chunks);
        }

        let cursor_display_cols: Vec<(usize, Style)> = lm
            .cursors
            .iter()
            .map(|cm| {
                let char_col = buffer_to_display(cm.col, &effective_conceals);
                let display_col = char_col_to_display_col(&full_line_text, char_col, &self.tab_display_unit);
                (display_col, cm.style)
            })
            .collect();

        let mut spans = segments.into_spans(
            self.h_scroll,
            width,
            &self.tab_display_unit,
            self.tab_style,
            &cursor_display_cols,
        );

        let cursors: Vec<(usize, CursorShape)> = cursor_display_cols
            .iter()
            .zip(lm.cursors.iter())
            .map(|(&(display_col, _), cm)| (display_col, cm.shape))
            .collect();

        append_eol_and_right_align(
            &mut spans,
            &mut lm.eol_chunks,
            &mut lm.right_align_chunks,
            width,
        );

        let mut popups = Vec::new();
        for pm in lm.popups {
            let display_col = buffer_to_display(pm.col, &effective_conceals);
            popups.push((display_col, pm.widget, pm.position, pm.priority));
        }

        LineRenderResult {
            line: Line::from(spans),
            cursors,
            popups,
        }
    }
}

/// State output from rendering a `TextBufferWidget`, carrying the cursor's screen position.
#[derive(Debug, Default)]
pub struct CursorRenderState {
    pub cursor: Option<(u16, u16, CursorShape)>,
}

struct LineRenderResult {
    line: Line<'static>,
    /// (display_col, shape) for each cursor on this line; caller converts to screen coords.
    cursors: Vec<(usize, CursorShape)>,
    /// (anchor_display_col, widget, position, z_index); caller computes screen coords.
    popups: Vec<(usize, Arc<dyn OverlayWidget>, OverlayPosition, i32)>,
}

struct StyledSegment {
    text: String,
    style: Style,
}

fn render_plain_line(
    line_str: &str,
    h_scroll: usize,
    width: usize,
    tab_display_unit: &str,
    tab_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut text = String::new();
    let mut col = 0usize;
    let tab_w = tab_display_unit.chars().count();
    for g in line_str.graphemes(true) {
        if g == "\t" {
            if col + tab_w <= h_scroll {
                col += tab_w;
                continue;
            }
            if col >= h_scroll + width {
                break;
            }
            if !text.is_empty() {
                spans.push(Span::raw(std::mem::take(&mut text)));
            }
            spans.push(Span::styled(tab_display_unit.to_owned(), tab_style));
            col += tab_w;
            continue;
        }
        if g == "\n" || g == "\r\n" || g == "\r" {
            break;
        }
        let g_w = grapheme_display_width(g);
        if col + g_w <= h_scroll {
            col += g_w;
            continue;
        }
        if col >= h_scroll + width {
            break;
        }
        text.push_str(g);
        col += g_w;
    }
    if !text.is_empty() {
        spans.push(Span::raw(text));
    }
    spans
}

fn char_to_byte_offset(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len())
}

fn char_col_to_display_col(line_text: &str, char_col: usize, tab_display_unit: &str) -> usize {
    let tab_w = tab_display_unit.chars().count();
    let mut display_col = 0usize;
    for (i, g) in line_text.graphemes(true).enumerate() {
        if i >= char_col {
            break;
        }
        if g == "\t" {
            display_col += tab_w;
        } else {
            display_col += grapheme_display_width(g);
        }
    }
    display_col
}

fn buffer_to_display(col: usize, conceals: &[EffectiveConceal<'_>]) -> usize {
    let mut shift: isize = 0;
    for ec in conceals {
        let buf_len = ec.end - ec.start;
        let rep_len = ec.replacement.map(|r| r.chars().count()).unwrap_or(0);
        let delta = rep_len as isize - buf_len as isize;
        if col < ec.start {
            break;
        } else if col < ec.end {
            return (ec.start as isize + shift) as usize;
        } else {
            shift += delta;
        }
    }
    (col as isize + shift) as usize
}

struct ColLocation {
    seg_idx: usize,
    char_offset: usize,
}

struct SegmentList {
    segments: Vec<StyledSegment>,
}

impl SegmentList {
    fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    fn push(&mut self, seg: StyledSegment) {
        self.segments.push(seg);
    }

    fn find_col(&self, display_col: usize, inclusive: bool) -> Option<ColLocation> {
        let mut visual_pos = 0usize;
        for (idx, seg) in self.segments.iter().enumerate() {
            let seg_len = seg.text.chars().count();
            let seg_visual_end = visual_pos + seg_len;
            let in_range = if inclusive {
                display_col >= visual_pos && display_col <= seg_visual_end
            } else {
                display_col >= visual_pos && display_col < seg_visual_end
            };
            if in_range {
                return Some(ColLocation {
                    seg_idx: idx,
                    char_offset: display_col - visual_pos,
                });
            }
            visual_pos = seg_visual_end;
        }
        None
    }

    fn insert_at(&mut self, display_col: usize, chunks: &[StyledChunk]) {
        if let Some(loc) = self.find_col(display_col, true) {
            let seg_text = std::mem::take(&mut self.segments[loc.seg_idx].text);
            let style = self.segments[loc.seg_idx].style;
            let seg_char_len = seg_text.chars().count();
            let byte_offset = char_to_byte_offset(&seg_text, loc.char_offset);

            let mut replacements: Vec<StyledSegment> = Vec::with_capacity(2 + chunks.len());
            if loc.char_offset > 0 {
                replacements.push(StyledSegment {
                    text: seg_text[..byte_offset].to_string(),
                    style,
                });
            }
            for chunk in chunks {
                replacements.push(StyledSegment {
                    text: chunk.text.clone(),
                    style: chunk.style,
                });
            }
            if loc.char_offset < seg_char_len {
                replacements.push(StyledSegment {
                    text: seg_text[byte_offset..].to_string(),
                    style,
                });
            }
            self.segments
                .splice(loc.seg_idx..=loc.seg_idx, replacements);
        } else {
            for chunk in chunks {
                self.segments.push(StyledSegment {
                    text: chunk.text.clone(),
                    style: chunk.style,
                });
            }
        }
    }

    fn overlay_at(&mut self, display_col: usize, chunks: &[StyledChunk]) {
        let overlay_text: String = chunks.iter().map(|c| c.text.as_str()).collect();
        let overlay_len = overlay_text.chars().count();
        if overlay_len == 0 {
            return;
        }

        let mut new_segments: Vec<StyledSegment> = Vec::new();
        let mut visual_pos = 0usize;
        let mut overlay_remaining = overlay_len;
        let mut chunk_idx = 0;
        let mut chunk_char_offset = 0;
        let overlay_start = display_col;
        let overlay_end = display_col + overlay_len;

        // Phase 1: walk existing segments, splitting each one that overlaps the overlay range.
        // Segments fully outside the range are kept as-is; overlapping segments are split into
        // a pre-part (original style), an overlay part (chunk style), and a post-part (original style).
        for seg in self.segments.drain(..) {
            let seg_len = seg.text.chars().count();
            let seg_end = visual_pos + seg_len;

            if seg_end <= overlay_start || visual_pos >= overlay_end {
                new_segments.push(seg);
            } else {
                if visual_pos < overlay_start {
                    let before_count = overlay_start - visual_pos;
                    let byte_end = char_to_byte_offset(&seg.text, before_count);
                    new_segments.push(StyledSegment {
                        text: seg.text[..byte_end].to_string(),
                        style: seg.style,
                    });
                }

                // Consume overlay chunks to fill the portion of this segment that is overlapped.
                let mut remaining_in_seg = seg_end.min(overlay_end) - visual_pos.max(overlay_start);
                while remaining_in_seg > 0 && chunk_idx < chunks.len() {
                    let chunk = &chunks[chunk_idx];
                    let chunk_char_len = chunk.text.chars().count();
                    let avail = chunk_char_len - chunk_char_offset;
                    let take = avail.min(remaining_in_seg);
                    let byte_start = char_to_byte_offset(&chunk.text, chunk_char_offset);
                    let byte_end = char_to_byte_offset(&chunk.text, chunk_char_offset + take);
                    new_segments.push(StyledSegment {
                        text: chunk.text[byte_start..byte_end].to_string(),
                        style: chunk.style,
                    });
                    chunk_char_offset += take;
                    remaining_in_seg -= take;
                    overlay_remaining -= take;
                    if chunk_char_offset >= chunk_char_len {
                        chunk_idx += 1;
                        chunk_char_offset = 0;
                    }
                }

                if seg_end > overlay_end {
                    let after_start = overlay_end - visual_pos;
                    let byte_start = char_to_byte_offset(&seg.text, after_start);
                    new_segments.push(StyledSegment {
                        text: seg.text[byte_start..].to_string(),
                        style: seg.style,
                    });
                }
            }
            visual_pos = seg_end;
        }

        // Phase 2: overlay extends past the last existing segment — append remaining overlay chars.
        while overlay_remaining > 0 && chunk_idx < chunks.len() {
            let chunk = &chunks[chunk_idx];
            let chunk_char_len = chunk.text.chars().count();
            let avail = chunk_char_len - chunk_char_offset;
            let take = avail.min(overlay_remaining);
            if take > 0 {
                let byte_start = char_to_byte_offset(&chunk.text, chunk_char_offset);
                let byte_end = char_to_byte_offset(&chunk.text, chunk_char_offset + take);
                new_segments.push(StyledSegment {
                    text: chunk.text[byte_start..byte_end].to_string(),
                    style: chunk.style,
                });
            }
            overlay_remaining -= take;
            chunk_char_offset += take;
            if chunk_char_offset >= chunk_char_len {
                chunk_idx += 1;
                chunk_char_offset = 0;
            }
        }

        self.segments = new_segments;
    }

    fn into_spans(
        self,
        h_scroll: usize,
        width: usize,
        tab_display_unit: &str,
        tab_style: Style,
        cursor_display_cols: &[(usize, Style)],
    ) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut display_col = 0usize;

        for seg in &self.segments {
            if display_col >= h_scroll + width {
                break;
            }

            let mut visible = String::new();
            for g in seg.text.graphemes(true) {
                if g == "\t" {
                    let unit_w = tab_display_unit.chars().count();
                    if display_col + unit_w <= h_scroll {
                        display_col += unit_w;
                        continue;
                    }
                    if display_col >= h_scroll + width {
                        break;
                    }
                    if !visible.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut visible), seg.style));
                    }
                    let style = if let Some(&(_, cursor_style)) =
                        cursor_display_cols.iter().find(|&&(col, _)| col == display_col)
                    {
                        cursor_style
                    } else {
                        seg.style.patch(tab_style)
                    };
                    spans.push(Span::styled(tab_display_unit.to_owned(), style));
                    display_col += unit_w;
                    continue;
                }
                let g_w = grapheme_display_width(g);
                if display_col + g_w <= h_scroll {
                    display_col += g_w;
                    continue;
                }
                if display_col >= h_scroll + width {
                    break;
                }
                visible.push_str(g);
                display_col += g_w;
            }

            if !visible.is_empty() {
                spans.push(Span::styled(visible, seg.style));
            }
        }

        spans
    }
}


struct ConcealMark<'a> {
    start: usize,
    end: usize,
    replacement: Option<&'a str>,
    style: Option<Style>,
    trim_before: bool,
    trim_after: bool,
    /// Carried during classify for suppression; stripped before passing downstream.
    ns: &'a str,
    scope: ConcealScope,
}

/// A conceal mark with whitespace trimming already applied, ready for segment building.
struct EffectiveConceal<'a> {
    start: usize,
    end: usize,
    replacement: Option<&'a str>,
    style: Option<Style>,
}

struct HighlightMark {
    start: usize,
    end: usize,
    style: Style,
    priority: i32,
}

struct CursorMark {
    col: usize,
    style: Style,
    shape: CursorShape,
}

struct ChunkMark<'a> {
    chunk: &'a StyledChunk,
    priority: i32,
}

struct ColChunksMark<'a> {
    col: usize,
    chunks: &'a [StyledChunk],
}

struct PopupMark {
    col: usize,
    widget: Arc<dyn OverlayWidget>,
    position: OverlayPosition,
    priority: i32,
}

struct SuppressRange<'a> {
    start: usize,
    end: usize,
    ns: &'a str,
}


struct LineMarks<'a> {
    conceals: Vec<ConcealMark<'a>>,
    highlights: Vec<HighlightMark>,
    cursors: Vec<CursorMark>,
    newline_highlights: Vec<(Style, i32)>,
    eol_highlights: Vec<(Style, i32)>,
    eol_chunks: Vec<ChunkMark<'a>>,
    overlay_marks: Vec<ColChunksMark<'a>>,
    inline_marks: Vec<ColChunksMark<'a>>,
    right_align_chunks: Vec<ChunkMark<'a>>,
    popups: Vec<PopupMark>,
}

impl<'a> LineMarks<'a> {
    fn classify(
        marks: &[&'a Extmark],
        line_start_char: usize,
        line_end_char: usize,
        line_char_count: usize,
        visible_len: usize,
        reveal_conceal_on_cursor_line: bool,
        ns_priority: &impl Fn(&str) -> i32,
    ) -> Self {
        let mut result = Self {
            conceals: Vec::new(),
            highlights: Vec::new(),
            cursors: Vec::new(),
            newline_highlights: Vec::new(),
            eol_highlights: Vec::new(),
            eol_chunks: Vec::new(),
            overlay_marks: Vec::new(),
            inline_marks: Vec::new(),
            right_align_chunks: Vec::new(),
            popups: Vec::new(),
        };

        let mut suppress_ranges: Vec<SuppressRange<'a>> = Vec::new();
        let mut cursor_on_line = false;

        for mark in marks {
            let mark_start_char = mark.byte_range.start;
            let mark_end_char = mark.byte_range.end;

            let priority = ns_priority(&mark.namespace);
            match &mark.kind {
                ExtmarkKind::Cursor { style, shape } => {
                    if mark_start_char >= line_start_char && mark_start_char <= line_end_char {
                        let col = mark_start_char - line_start_char;
                        result.cursors.push(CursorMark { col, style: *style, shape: *shape });
                        if col >= visible_len {
                            if visible_len == 0 {
                                result.newline_highlights.push((*style, priority));
                            } else {
                                result.eol_highlights.push((*style, priority));
                            }
                        }
                        suppress_ranges.push(SuppressRange { start: col, end: col + 1, ns: &mark.namespace });
                        cursor_on_line = true;
                    }
                }
                ExtmarkKind::Highlight { style } => {
                    let start_col = mark_start_char.saturating_sub(line_start_char);
                    let end_col = if mark_end_char <= line_end_char {
                        mark_end_char - line_start_char
                    } else {
                        line_char_count
                    };

                    if (end_col == start_col && end_col <= visible_len)
                        || (visible_len == 0 && start_col == 0 && end_col >= 1)
                    {
                        result.newline_highlights.push((*style, priority));
                    } else if start_col >= visible_len && visible_len > 0 {
                        result.eol_highlights.push((*style, priority));
                    } else if end_col > start_col {
                        result.highlights.push(HighlightMark { start: start_col, end: end_col, style: *style, priority });
                        suppress_ranges.push(SuppressRange { start: start_col, end: end_col, ns: &mark.namespace });
                    }
                }
                ExtmarkKind::VirtualText { chunks, pos } => {
                    if mark_start_char >= line_start_char && mark_start_char <= line_end_char {
                        let col = mark_start_char - line_start_char;
                        match pos {
                            VirtTextPos::Eol => {
                                for chunk in chunks {
                                    result.eol_chunks.push(ChunkMark { chunk, priority });
                                }
                            }
                            VirtTextPos::Overlay => {
                                result.overlay_marks.push(ColChunksMark { col, chunks: chunks.as_slice() });
                            }
                            VirtTextPos::Inline => {
                                result.inline_marks.push(ColChunksMark { col, chunks: chunks.as_slice() });
                            }
                            VirtTextPos::RightAlign => {
                                for chunk in chunks {
                                    result.right_align_chunks.push(ChunkMark { chunk, priority });
                                }
                            }
                        }
                    }
                }
                ExtmarkKind::Conceal { replacement, style, scope, trim_before, trim_after } => {
                    if mark_start_char >= line_start_char && mark_start_char < line_end_char {
                        let start_col = mark_start_char - line_start_char;
                        let end_col = (mark_end_char - line_start_char).min(visible_len);
                        result.conceals.push(ConcealMark {
                            start: start_col,
                            end: end_col,
                            replacement: replacement.as_deref(),
                            style: *style,
                            trim_before: *trim_before,
                            trim_after: *trim_after,
                            ns: &mark.namespace,
                            scope: *scope,
                        });
                    }
                }
                ExtmarkKind::Overlay { widget, position } => {
                    if mark_start_char >= line_start_char && mark_start_char <= line_end_char {
                        let col = mark_start_char - line_start_char;
                        result.popups.push(PopupMark { col, widget: widget.clone(), position: position.clone(), priority });
                    }
                }
            }
        }

        // Drop conceals suppressed by marks from a different namespace.
        if !result.conceals.is_empty() {
            let force_line_scope = reveal_conceal_on_cursor_line && cursor_on_line;
            result.conceals.retain(|cm| {
                let effective_scope = if force_line_scope { ConcealScope::Line } else { cm.scope };
                match effective_scope {
                    ConcealScope::Byte => !suppress_ranges
                        .iter()
                        .any(|sr| sr.ns != cm.ns && sr.start < cm.end && sr.end > cm.start),
                    ConcealScope::Line => !suppress_ranges.iter().any(|sr| sr.ns != cm.ns),
                }
            });
        }

        result.conceals.sort_by_key(|cm| cm.start);
        result
    }
}

fn build_concealed_segments<'a>(
    line_text: &'a str,
    conceals: &[EffectiveConceal<'a>],
) -> (SegmentList, Vec<(usize, usize)>) {
    let mut segments = SegmentList::new();
    let mut col_ranges: Vec<(usize, usize)> = Vec::new();
    let mut char_pos = 0usize;
    let total_chars = line_text.chars().count();

    if conceals.is_empty() {
        segments.push(StyledSegment {
            text: line_text.to_string(),
            style: Style::default(),
        });
        return (segments, col_ranges);
    }

    let chars: Vec<(usize, char)> = line_text.char_indices().collect();

    for ec in conceals {
        if char_pos < ec.start && char_pos < total_chars {
            let from_byte = chars
                .get(char_pos)
                .map(|(b, _)| *b)
                .unwrap_or(line_text.len());
            let to_byte = chars
                .get(ec.start)
                .map(|(b, _)| *b)
                .unwrap_or(line_text.len());
            if from_byte < to_byte {
                segments.push(StyledSegment {
                    text: line_text[from_byte..to_byte].to_string(),
                    style: Style::default(),
                });
            }
        }

        let display_start = char_pos.min(ec.start);
        let rep_len = ec.replacement.map(|r| r.chars().count()).unwrap_or(0);

        if let Some(rep) = ec.replacement {
            segments.push(StyledSegment {
                text: rep.to_string(),
                style: ec.style.unwrap_or_default(),
            });
        }

        let display_end = display_start + rep_len;
        col_ranges.push((display_start, display_end));

        char_pos = ec.end;
    }

    if char_pos < total_chars {
        let from_byte = chars
            .get(char_pos)
            .map(|(b, _)| *b)
            .unwrap_or(line_text.len());
        segments.push(StyledSegment {
            text: line_text[from_byte..].to_string(),
            style: Style::default(),
        });
    }

    (segments, col_ranges)
}

fn apply_highlights(
    mut segments: SegmentList,
    _col_ranges: &[(usize, usize)],
    highlights: &mut [HighlightMark],
    conceals: &[EffectiveConceal<'_>],
    newline_highlights: &[(Style, i32)],
    visible_len: usize,
) -> SegmentList {
    highlights.sort_by_key(|h| h.priority);

    for hl in highlights.iter() {
        let hl_start = hl.start;
        let hl_end = hl.end;
        let style = hl.style;
        let display_start = buffer_to_display(hl_start, conceals);
        let display_end = buffer_to_display(hl_end, conceals);
        if display_start >= display_end {
            continue;
        }

        let mut new_segments = SegmentList::new();
        let mut char_pos = 0usize;

        for seg in segments.segments.drain(..) {
            let seg_len = seg.text.chars().count();
            let seg_end = char_pos + seg_len;

            let overlap_start = display_start.max(char_pos);
            let overlap_end = display_end.min(seg_end);

            if overlap_start >= overlap_end {
                new_segments.push(seg);
            } else {
                if overlap_start > char_pos {
                    let byte_end = char_to_byte_offset(&seg.text, overlap_start - char_pos);
                    new_segments.push(StyledSegment {
                        text: seg.text[..byte_end].to_string(),
                        style: seg.style,
                    });
                }
                let byte_start = char_to_byte_offset(&seg.text, overlap_start - char_pos);
                let byte_end = char_to_byte_offset(&seg.text, overlap_end - char_pos);
                new_segments.push(StyledSegment {
                    text: seg.text[byte_start..byte_end].to_string(),
                    style: seg.style.patch(style),
                });
                if overlap_end < seg_end {
                    let byte_start = char_to_byte_offset(&seg.text, overlap_end - char_pos);
                    new_segments.push(StyledSegment {
                        text: seg.text[byte_start..].to_string(),
                        style: seg.style,
                    });
                }
            }

            char_pos = seg_end;
        }

        segments = new_segments;
    }

    if !newline_highlights.is_empty() && visible_len == 0 {
        let mut nl_sorted: Vec<_> = newline_highlights.to_vec();
        nl_sorted.sort_by_key(|(_, p)| *p);
        let mut style = Style::default();
        for (s, _) in &nl_sorted {
            style = style.patch(*s);
        }
        segments.push(StyledSegment {
            text: " ".to_string(),
            style,
        });
    }

    segments
}

fn append_eol_and_right_align(
    spans: &mut Vec<Span<'static>>,
    eol_chunks: &mut Vec<ChunkMark<'_>>,
    right_align_chunks: &mut Vec<ChunkMark<'_>>,
    width: usize,
) {
    eol_chunks.sort_by_key(|cm| cm.priority);
    for cm in eol_chunks.iter() {
        spans.push(Span::styled(cm.chunk.text.clone(), cm.chunk.style));
    }

    if !right_align_chunks.is_empty() {
        right_align_chunks.sort_by_key(|cm| cm.priority);
        let current_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let right_len: usize = right_align_chunks.iter().map(|cm| cm.chunk.text.chars().count()).sum();
        if current_len + right_len < width {
            let padding = width - current_len - right_len;
            spans.push(Span::raw(" ".repeat(padding)));
        }
        for cm in right_align_chunks.iter() {
            spans.push(Span::styled(cm.chunk.text.clone(), cm.chunk.style));
        }
    }
}

impl<'a> StatefulWidget for TextBufferWidget<'a> {
    type State = CursorRenderState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State)
    where
        Self: Sized,
    {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let rope = &self.buf.rope;
        let mut lines = vec![];
        let mut pending_overlays = vec![];

        let total_lines = rope.len_lines();

        let viewport_start_byte = if self.line_scroll < total_lines {
            rope.char_to_byte(rope.line_to_char(self.line_scroll))
        } else {
            rope.len_bytes()
        };
        let viewport_end_line = (self.line_scroll + area.height as usize).min(total_lines);
        let viewport_end_byte = if viewport_end_line < total_lines {
            rope.char_to_byte(rope.line_to_char(viewport_end_line))
        } else {
            rope.len_bytes()
        };

        let all_viewport_marks = self
            .buf
            .renderer
            .query_extmarks(viewport_start_byte..viewport_end_byte + 1);

        while lines.len() < area.height as usize {
            let line_idx = self.line_scroll + lines.len();
            let Some(rope_line) = rope.get_line(line_idx) else {
                break;
            };

            let line_start_byte = rope.char_to_byte(rope.line_to_char(line_idx));
            let line_start_char = rope.byte_to_char(line_start_byte);
            let line_char_count = rope_line.len_chars();
            let line_end_char = line_start_char + line_char_count;

            let line_end_byte = if line_end_char <= rope.len_chars() {
                rope.char_to_byte(line_end_char)
            } else {
                rope.len_bytes()
            };

            let visible_len = {
                let mut vl = line_char_count;
                if vl > 0 && rope_line.char(vl - 1) == '\n' {
                    vl -= 1;
                }
                if vl > 0 && rope_line.char(vl - 1) == '\r' {
                    vl -= 1;
                }
                vl
            };

            // Last line with no trailing newline — render one extra space so cursors
            // at the EOF position have a cell to occupy.
            let extra_eof_space =
                line_end_byte == rope.len_bytes() && visible_len == line_char_count;

            let marks: Vec<&Extmark> = all_viewport_marks
                .iter()
                .filter(|mark| {
                    let mark_start = mark.byte_range.start;
                    let mark_end = mark.byte_range.end;
                    (mark_start < line_end_byte
                        || (mark_start == line_end_byte && line_start_byte == line_end_byte)
                        || (extra_eof_space && mark_start == line_end_byte))
                        && mark_end >= line_start_byte
                })
                .copied()
                .collect();

            if marks.is_empty() {
                let line_str = rope_line.to_string();
                let spans = render_plain_line(
                    &line_str,
                    self.h_scroll,
                    area.width as usize,
                    &self.tab_display_unit,
                    self.tab_style,
                );
                lines.push(Line::from(spans));
                continue;
            }

            let result = self.render_marked_line(
                rope,
                &marks,
                rope_line,
                line_start_char,
                line_end_char,
                line_char_count,
                visible_len,
                extra_eof_space,
                area.width as usize,
                self.reveal_conceal_on_cursor_line,
            );

            let current_line_index = lines.len();
            for (display_col, shape) in &result.cursors {
                if *display_col >= self.h_scroll
                    && *display_col < self.h_scroll + area.width as usize
                {
                    let screen_x = area.x + (display_col - self.h_scroll) as u16;
                    let screen_y = area.y + current_line_index as u16;
                    state.cursor = Some((screen_x, screen_y, *shape));
                }
            }

            for (anchor_display_col, content, position, z_index) in result.popups {
                let screen_x = if anchor_display_col >= self.h_scroll {
                    area.x + (anchor_display_col - self.h_scroll) as u16
                } else {
                    area.x
                };
                let screen_y = area.y + current_line_index as u16;
                pending_overlays.push((screen_x, screen_y, content, position, z_index));
            }

            lines.push(result.line);
        }

        Text::from(lines).render(area, buf);

        pending_overlays.sort_by_key(|(_, _, _, _, z)| *z);
        for (anchor_x, anchor_y, widget, position, _) in pending_overlays {
            let (w, h) = widget.dimensions();
            let (offset_x, offset_y) = match position {
                OverlayPosition::Fixed { offset_x, offset_y } => (offset_x, offset_y),
                OverlayPosition::Smart => {
                    let rows_below = (area.y + area.height).saturating_sub(anchor_y + 1);
                    let oy = if rows_below >= h { 1 } else { -(h as i32) };
                    let overflow_right = (anchor_x as i32 + w as i32)
                        .saturating_sub((area.x + area.width) as i32)
                        .max(0);
                    let ox = -(overflow_right.min((anchor_x - area.x) as i32));
                    (ox, oy)
                }
            };
            let dst_x0 = (anchor_x as i32 + offset_x).max(area.x as i32) as u16;
            let dst_y0 = (anchor_y as i32 + offset_y).max(area.y as i32) as u16;
            let avail_w = (area.x + area.width).saturating_sub(dst_x0).min(w);
            let avail_h = (area.y + area.height).saturating_sub(dst_y0).min(h);
            if avail_w == 0 || avail_h == 0 {
                continue;
            }
            let avail_rect = Rect::new(0, 0, avail_w, avail_h);
            let mut tmp_buf = ratatui::buffer::Buffer::empty(avail_rect);
            widget.render(avail_rect, &mut tmp_buf);
            for cy in 0..avail_h {
                for cx in 0..avail_w {
                    if let (Some(src), Some(dst)) = (
                        tmp_buf.cell((cx, cy)),
                        buf.cell_mut((dst_x0 + cx, dst_y0 + cy)),
                    ) {
                        *dst = src.clone();
                    }
                }
            }
        }
    }
}
