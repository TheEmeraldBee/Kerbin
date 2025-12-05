use std::collections::HashMap;

use crate::*;
use ascii_forge::prelude::*;
use serde::Deserialize;

/// Configuration for a specific mode within the statusline.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct ModeConfig {
    /// An optional longer, more descriptive name for the mode (e.g., "NORMAL" instead of "n").
    pub long_name: Option<String>,

    /// An optional key to retrieve a specific `ContentStyle` from the `Theme` for this mode.
    pub theme_key: Option<String>,
}

/// Overall configuration for the editor's statusline.
#[derive(Deserialize, Default, Debug)]
pub struct StatuslineConfig {
    pub modes: HashMap<char, ModeConfig>,
}

/// Renders the editor's statusline, displaying current modes, cursor information, and other details.
pub async fn render_statusline(
    chunk: Chunk<StatuslineChunk>,
    plugin_config: Res<PluginConfig>,
    theme: Res<Theme>,
    mode_stack: Res<ModeStack>,

    input: Res<InputState>,

    buffers: Res<Buffers>,
) {
    // Deserialize statusline-specific configuration from the plugin config
    let plugin_config = plugin_config
        .get()
        .await
        .0
        .get("statusline")
        .map(|x| StatuslineConfig::deserialize(x.clone()).unwrap())
        .unwrap_or_default();

    get!(Some(mut chunk), theme, mode_stack, input);

    let chunk_width = chunk.size().x;

    let mut parts = vec![];

    // Build the mode display parts for the statusline
    for part in &mode_stack.0 {
        if let Some(config) = plugin_config.modes.get(part) {
            parts.push((
                config.long_name.clone().unwrap_or(part.to_string()),
                config
                    .theme_key
                    .clone()
                    .and_then(|x| theme.get(&x))
                    .unwrap_or_else(|| {
                        // Fallback theme if specific key not found or not provided
                        theme.get_fallback_default([
                            format!("statusline.mode.{part}"),
                            "statusline.mode".to_string(),
                        ])
                    }),
            ))
        } else {
            // Default display for modes without specific configuration
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

    let mut loc = vec2(0, 0);

    // Render the mode parts
    for (i, (text, theme)) in parts.into_iter().enumerate().take(4) {
        if loc.x >= chunk.size().x {
            // Stop rendering if beyond chunk width
            break;
        }
        // Render each part, adding " -> " separator if it's not the first part
        loc = render!(chunk, loc => [if i != 0 {" -> "} else {""}, theme.apply(text)]);
    }

    // Build right-aligned content
    let cur_buf = buffers.get().await.cur_buffer().await;
    let mut right_parts = vec![];

    // Add repeat count if present
    if !input.repeat_count.is_empty() {
        let repeat_style = theme.get_fallback_default(["statusline.repeat"]);
        tracing::error!("{}", input.repeat_count);
        right_parts.push(repeat_style.apply(input.repeat_count.clone()));
    }

    // Add cursor/selection count
    let cursor_count = cur_buf.cursors.len();
    if cursor_count == 1 {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.one", "statusline.selections"]);
        right_parts.push(sel_style.apply("1 sel".to_string()));
    } else {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.multi", "statusline.selections"]);
        let primary_cursor = cur_buf.primary_cursor + 1; // Display as 1-indexed
        right_parts.push(sel_style.apply(format!("{}/{} sels", primary_cursor, cursor_count)));
    }

    // Calculate total width needed for right-aligned content
    let spacing = if right_parts.len() > 1 { 3 } else { 0 }; // " | " separator
    let right_width: usize = right_parts.iter().map(|s| s.content().len()).sum::<usize>() + spacing;

    // Render right-aligned content if it fits
    if right_width <= chunk_width as usize {
        let mut right_loc = vec2(chunk_width.saturating_sub(right_width as u16), 0);

        for (i, part) in right_parts.into_iter().enumerate() {
            if i != 0 {
                right_loc = render!(chunk, right_loc => [" | "]);
            }
            right_loc = render!(chunk, right_loc => [part]);
        }
    }
}
