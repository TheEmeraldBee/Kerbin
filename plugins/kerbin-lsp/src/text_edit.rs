use kerbin_core::*;
use lsp_types::TextEdit;
use ropey::RopeSlice;

/// Applies a list of LSP text edits to a buffer without wrapping in a change group.
/// The caller is responsible for calling `start_change_group` / `commit_change_group`.
///
/// Edits are sorted descending by start position so earlier byte offsets remain valid
/// as edits are applied sequentially.
pub(crate) fn apply_text_edits_inner(buf: &mut TextBuffer, mut edits: Vec<TextEdit>) {
    if edits.is_empty() {
        return;
    }

    edits.sort_by(|a, b| {
        (b.range.start.line, b.range.start.character)
            .cmp(&(a.range.start.line, a.range.start.character))
    });

    let line_content_len = |line_slice: &RopeSlice<'_>| {
        let mut len = line_slice.len_chars();
        if len > 0 {
            match line_slice.char(len - 1) {
                '\n' => {
                    len -= 1;
                    if len > 0 && line_slice.char(len - 1) == '\r' {
                        len -= 1;
                    }
                }
                '\r' => len -= 1,
                _ => {}
            }
        }
        len
    };

    for edit in &edits {
        let max_line = buf.len_lines().saturating_sub(1);
        let start_line = (edit.range.start.line as usize).min(max_line);
        let end_line = (edit.range.end.line as usize).min(max_line);

        let start_line_slice = buf.line_clamped(start_line);
        let start_char =
            (edit.range.start.character as usize).min(line_content_len(&start_line_slice));
        let start_byte =
            buf.line_to_byte_clamped(start_line) + start_line_slice.char_to_byte(start_char);

        let end_line_slice = buf.line_clamped(end_line);
        let end_char =
            (edit.range.end.character as usize).min(line_content_len(&end_line_slice));
        let end_byte =
            buf.line_to_byte_clamped(end_line) + end_line_slice.char_to_byte(end_char);

        let del_chars =
            buf.byte_to_char_clamped(end_byte) - buf.byte_to_char_clamped(start_byte);
        if del_chars > 0 {
            buf.action(Delete {
                byte: start_byte,
                len: del_chars,
            });
        }
        if !edit.new_text.is_empty() {
            buf.action(Insert {
                byte: start_byte,
                content: edit.new_text.clone(),
            });
        }
    }
}

/// Applies a list of LSP text edits to a buffer as a single atomic undo group.
pub fn apply_text_edits(buf: &mut TextBuffer, edits: Vec<TextEdit>) {
    if edits.is_empty() {
        return;
    }
    buf.start_change_group();
    apply_text_edits_inner(buf, edits);
    buf.commit_change_group();
}
