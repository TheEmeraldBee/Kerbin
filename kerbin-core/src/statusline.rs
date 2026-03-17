use std::collections::HashMap;

use crate::*;
use ratatui::prelude::*;
use serde::Deserialize;

/// Configuration for a specific mode within the statusline
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct ModeConfig {
    /// An optional longer, more descriptive name for the mode
    pub long_name: Option<String>,

    /// An optional key to retrieve a specific `Style` from the `Theme` for this mode
    pub theme_key: Option<String>,
}

/// Overall configuration for the editor's statusline
#[derive(Deserialize, Default, Debug, State)]
pub struct StatuslineConfig {
    pub modes: HashMap<char, ModeConfig>,
}

pub async fn render_statusline(
    chunk: Chunk<StatuslineChunk>,
    statusline_config: Res<StatuslineConfig>,
    theme: Res<Theme>,
    mode_stack: Res<ModeStack>,

    input: Res<InputState>,

    buffers: Res<Buffers>,
) {
    get!(statusline_config, Some(mut chunk), theme, mode_stack, input);

    let chunk_width = chunk.area().width;
    let base_x = chunk.area().x;
    let base_y = chunk.area().y;

    let mut parts = vec![];

    // Build the mode display parts for the statusline
    for part in &mode_stack.0 {
        if let Some(config) = statusline_config.modes.get(part) {
            parts.push((
                config.long_name.clone().unwrap_or(part.to_string()),
                config
                    .theme_key
                    .clone()
                    .and_then(|x| theme.get(&x))
                    .unwrap_or_else(|| {
                        theme.get_fallback_default([
                            format!("statusline.mode.{part}"),
                            "statusline.mode".to_string(),
                        ])
                    }),
            ))
        } else {
            parts.push((
                part.to_string(),
                theme.get_fallback_default([
                    format!("statusline.mode.{part}"),
                    "statusline.mode".to_string(),
                ]),
            ))
        }
    }

    // Condense the mode stack display if it's too long
    if parts.len() > 3 {
        parts.insert(
            0,
            (
                "...".to_string(),
                theme.get_fallback_default(["statusline.mode.etc", "statusline.mode"]),
            ),
        );
    }

    let mut x: u16 = 0;

    // Render the mode parts
    for (i, (text, style)) in parts.into_iter().enumerate().take(4) {
        if x >= chunk_width {
            break;
        }
        let prefix = if i != 0 { " -> " } else { "" };
        if !prefix.is_empty() {
            chunk.set_string(base_x + x, base_y, prefix, Style::default());
            x += prefix.chars().count() as u16;
        }
        chunk.set_string(base_x + x, base_y, &text, style);
        x += text.chars().count() as u16;
    }

    // Build right-aligned content
    let cur_buf = buffers.get().await.cur_buffer().await;
    let mut right_parts: Vec<(String, Style)> = vec![];

    // Add repeat count if present
    if !input.repeat_count.is_empty() {
        let repeat_style = theme.get_fallback_default(["statusline.repeat"]);
        right_parts.push((input.repeat_count.clone(), repeat_style));
    }

    // Add cursor/selection count
    let cursor_count = cur_buf.cursors.len();
    if cursor_count == 1 {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.one", "statusline.selections"]);
        right_parts.push(("1 sel".to_string(), sel_style));
    } else {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.multi", "statusline.selections"]);
        let primary_cursor = cur_buf.primary_cursor + 1;
        right_parts.push((format!("{}/{} sels", primary_cursor, cursor_count), sel_style));
    }

    // Calculate total width needed for right-aligned content
    let spacing = if right_parts.len() > 1 { 3 } else { 0 }; // " | " separator
    let right_width: usize = right_parts.iter().map(|(s, _)| s.chars().count()).sum::<usize>() + spacing;

    // Render right-aligned content if it fits
    if right_width <= chunk_width as usize {
        let mut right_x = chunk_width.saturating_sub(right_width as u16);

        for (i, (text, style)) in right_parts.into_iter().enumerate() {
            if i != 0 {
                chunk.set_string(base_x + right_x, base_y, " | ", Style::default());
                right_x += 3;
            }
            chunk.set_string(base_x + right_x, base_y, &text, style);
            right_x += text.chars().count() as u16;
        }
    }
}
