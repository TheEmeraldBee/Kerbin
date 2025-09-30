use crate::*;
use ascii_forge::prelude::*;

/// Builds out the rendered lines for the current buffer, only building the required sizes
pub async fn build_buffer_lines(chunk: Chunk<BufferChunk>, bufs: Res<Buffers>, theme: Res<Theme>) {
    let Some(chunk) = chunk.get() else { return };
    let height = chunk.size().y;

    get!(bufs, theme);
    let buf = bufs.cur_buffer();
    let mut buf = buf.write().unwrap();

    let default_style = theme
        .get("ui.text")
        .unwrap_or_else(|| ContentStyle::new().with(Color::Rgb { r: 0, g: 0, b: 0 }));

    let mut lines = vec![];

    let mut line_idx = buf.renderer.byte_scroll;

    let mut byte_offset = buf
        .rope
        .line_to_byte_idx(buf.renderer.byte_scroll, LineType::LF_CR);

    let mut cursor = None;

    for line in buf.rope.lines_at(buf.renderer.byte_scroll, LineType::LF_CR) {
        // Once enough lines were rendered, exit out
        if lines.len() >= height as usize {
            break;
        }

        let mut render = RenderLine::default();

        let mut line_chars: Vec<(usize, char)> = line.char_indices().collect();
        let line_start_byte = buf.rope.line_to_byte_idx(line_idx, LineType::LF_CR);
        let line_end_byte = buf.rope.line_to_byte_idx(line_idx + 1, LineType::LF_CR);

        if let Some((_, ch)) = line_chars.last()
            && (*ch == '\n' || *ch == '\r')
        {
            line_chars.pop();
        }

        if line_chars.is_empty() {
            line_chars.push((0, ' '));
        } else {
            line_chars.push((line.len().saturating_sub(1).max(1), ' '));
        }

        let exts = buf.renderer.query_extmarks(line_start_byte..line_end_byte);

        for (byte, ch) in line_chars.into_iter() {
            let absolute_byte_idx = byte_offset + byte;

            let mut style = default_style;

            let mut after_elems = vec![];

            for ext in &exts {
                for deco in &ext.decorations {
                    match deco {
                        ExtmarkDecoration::Cursor { style } => {
                            if ext.byte_range.start == absolute_byte_idx {
                                cursor = Some((absolute_byte_idx, *style));
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
                    }
                }
            }

            // Add the resulting character to the buffer
            render.element(RenderLineElement::RopeChar(ch, absolute_byte_idx, style));

            for elem in after_elems {
                render.element(elem);
            }
        }

        line_idx += 1;
        byte_offset += line.len();
        lines.push(render);
    }

    buf.renderer.cursor = cursor;
    buf.renderer.lines = lines;
}
