use crate::*;
use ascii_forge::{prelude::*, window::crossterm::cursor::SetCursorStyle};
use unicode_segmentation::UnicodeSegmentation;

fn process_extmarks(
    exts: &[&Extmark],
    absolute_byte_idx: usize,
    default_style: ContentStyle,
    cursor: &mut Option<(usize, SetCursorStyle)>,
) -> (ContentStyle, Vec<RenderLineElement>, Vec<RenderLine>) {
    let mut style = default_style;
    let mut after_elems = vec![];
    let mut post_line_elems = vec![];

    for ext in exts {
        for deco in &ext.decorations {
            match deco {
                ExtmarkDecoration::Cursor {
                    style: cursor_style,
                } => {
                    if ext.byte_range.start == absolute_byte_idx {
                        *cursor = Some((absolute_byte_idx, *cursor_style));
                    }
                }
                ExtmarkDecoration::Highlight { hl } => {
                    if ext.byte_range.contains(&absolute_byte_idx) {
                        style = style.combined_with(hl);
                    }
                }
                ExtmarkDecoration::VirtText { text, hl } => {
                    if ext.byte_range.start == absolute_byte_idx {
                        after_elems.push(RenderLineElement::Text(
                            text.clone(),
                            hl.unwrap_or(ContentStyle::new().dark_grey()),
                        ));
                    }
                }
                ExtmarkDecoration::OverlayElement {
                    offset,
                    elem,
                    z_index,
                    clip_to_viewport,
                    positioning,
                } => {
                    if ext.byte_range.start == absolute_byte_idx {
                        after_elems.push(RenderLineElement::OverlayElement {
                            anchor_byte: absolute_byte_idx,
                            offset: *offset,
                            elem: elem.clone(),
                            z_index: *z_index,
                            clip_to_viewport: *clip_to_viewport,
                            positioning: *positioning,
                        });
                    }
                }
                ExtmarkDecoration::FullElement { elem } => {
                    if ext.byte_range.start == absolute_byte_idx {
                        post_line_elems.push(
                            RenderLine::default()
                                .with_element(RenderLineElement::Element(elem.clone())),
                        );
                        for _ in 1..elem.size().y {
                            post_line_elems.push(RenderLine::default().with_element(
                                RenderLineElement::ReservedSpace(elem.size().x as usize),
                            ))
                        }
                    }
                }
            }
        }
    }

    (style, after_elems, post_line_elems)
}

pub async fn build_buffer_lines(
    chunk: Chunk<BufferChunk>,
    bufs: ResMut<Buffers>,
    theme: Res<Theme>,
) {
    let Some(chunk) = chunk.get().await else {
        return;
    };
    let height = chunk.size().y;
    let viewport_height = height as usize;

    get!(mut bufs, theme);
    let mut buf = bufs.cur_buffer_mut().await;

    let default_style = theme
        .get("ui.text")
        .unwrap_or_else(|| ContentStyle::new().with(Color::Rgb { r: 0, g: 0, b: 0 }));

    let line_style = theme.get_fallback_default(["ui.linenum", "ui.text"]);

    let mut lines = vec![];

    let mut line_idx = buf.renderer.byte_scroll;

    let mut byte_offset = buf.line_to_byte_clamped(buf.renderer.byte_scroll);

    let mut cursor = None;

    let total_lines = buf.len_lines();
    let mut visual_lines = 0;

    // Iterate through lines starting from the scroll position
    for line in buf.lines_at_clamped(buf.renderer.byte_scroll) {
        if visual_lines >= viewport_height {
            break;
        }

        let mut render = RenderLine::default();
        render.element(RenderLineElement::Text(
            format!(" {:<3} ", line_idx + 1),
            line_style,
        ));

        // Calculate byte range for the current line
        let line_start_byte = buf.line_to_byte_clamped(line_idx);
        let line_end_byte = buf.line_to_byte_clamped(line_idx + 1);

        // Collect grapheme clusters with their char offsets.
        let line_str = line.to_string();
        let mut char_offset = 0usize;
        let mut line_graphemes: Vec<(usize, &str)> = line_str
            .graphemes(true)
            .map(|g| {
                let off = char_offset;
                char_offset += g.chars().count();
                (off, g)
            })
            .collect();

        // Strip trailing newline graphemes (CRLF or LF).
        if let Some((_, g)) = line_graphemes.last()
            && (*g == "\r\n" || *g == "\n" || *g == "\r")
        {
            line_graphemes.pop();
        }

        let is_last_line = line_idx == total_lines.saturating_sub(1);

        let sentinel_char_off = if line_graphemes.is_empty() {
            0
        } else if is_last_line {
            line.len_chars()
        } else {
            line.len_chars().saturating_sub(1).max(1)
        };

        let exts = buf
            .renderer
            .query_extmarks(line_start_byte..line_end_byte + 1);

        let mut post_line_elems = vec![];

        // Render a placeholder space when the line is empty so the cursor has a cell.
        if line_graphemes.is_empty() {
            let absolute_byte_idx = byte_offset + buf.char_to_byte_clamped(0);
            let (style, after_elems, mut post_elems) =
                process_extmarks(&exts, absolute_byte_idx, default_style, &mut cursor);
            post_line_elems.append(&mut post_elems);
            render.element(RenderLineElement::RopeChar(
                " ".to_string(),
                absolute_byte_idx,
                style,
            ));
            for elem in after_elems {
                render.element(elem);
            }
        } else {
            for (char_off, g) in line_graphemes.into_iter() {
                let absolute_byte_idx = byte_offset + buf.char_to_byte_clamped(char_off);

                let (style, after_elems, mut post_elems) =
                    process_extmarks(&exts, absolute_byte_idx, default_style, &mut cursor);

                post_line_elems.append(&mut post_elems);

                render.element(RenderLineElement::RopeChar(
                    g.to_string(),
                    absolute_byte_idx,
                    style,
                ));

                for elem in after_elems {
                    render.element(elem);
                }
            }

            // Sentinel space for cursor-at-EOL / EOF.
            let absolute_byte_idx = byte_offset + buf.char_to_byte_clamped(sentinel_char_off);
            let (style, after_elems, mut post_elems) =
                process_extmarks(&exts, absolute_byte_idx, default_style, &mut cursor);
            post_line_elems.append(&mut post_elems);
            render.element(RenderLineElement::RopeChar(
                " ".to_string(),
                absolute_byte_idx,
                style,
            ));
            for elem in after_elems {
                render.element(elem);
            }
        }

        line_idx += 1;
        // line.len_bytes() on RopeSlice gives byte length
        byte_offset += line.chunks().map(|c| c.len()).sum::<usize>();
        lines.push(render);
        visual_lines += 1;

        visual_lines += post_line_elems.len();
        lines.extend(post_line_elems);
    }

    buf.renderer.cursor = cursor;
    buf.renderer.lines = lines;
}
