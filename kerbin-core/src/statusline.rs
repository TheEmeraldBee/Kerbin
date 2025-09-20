use std::collections::HashMap;

use crate::*;
use ascii_forge::prelude::*;
use serde::Deserialize;

/// Configuration for a specific mode within the statusline.
///
/// This struct allows customization of how individual editor modes are displayed
/// in the statusline, including their long names and theme keys.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct ModeConfig {
    /// An optional longer, more descriptive name for the mode (e.g., "NORMAL" instead of "n").
    /// If `None`, the single-character mode identifier will be used.
    pub long_name: Option<String>,

    /// An optional key to retrieve a specific `ContentStyle` from the `Theme` for this mode.
    /// If `None`, a default fallback mechanism will be used.
    pub theme_key: Option<String>,

    /// If `true`, this mode's display in the statusline will hide all other modes.
    /// This is not currently implemented in `render_statusline` but is part of the config.
    pub hide_others: bool,
}

/// Overall configuration for the editor's statusline.
///
/// This struct holds a mapping of mode characters to their `ModeConfig`,
/// allowing detailed customization of each mode's statusline appearance.
#[derive(Deserialize, Default, Debug)]
pub struct StatuslineConfig {
    /// A hash map where keys are single-character mode identifiers (e.g., 'n', 'i', 'v')
    /// and values are `ModeConfig` instances defining how each mode should be displayed.
    pub modes: HashMap<char, ModeConfig>,
}

/// Renders the editor's statusline, displaying current modes, cursor information, and other details.
///
/// This asynchronous function takes various resources from the Kerbin state machine
/// to construct and render the statusline. It dynamically adjusts the display based
/// on the active modes, theme configuration, and current buffer state.
///
/// # Arguments
///
/// * `chunk`: A `Chunk` resource representing the drawing area for the statusline.
///            It holds an `Arc<RwLock<InnerChunk>>` for the `StatuslineChunk`.
/// * `plugin_config`: A `Res` (resource) holding the `PluginConfig`, which is used
///                    to extract statusline-specific configurations.
/// * `theme`: A `Res` holding the `Theme` resource, used to apply styling to the statusline elements.
/// * `mode_stack`: A `Res` holding the `ModeStack` resource, indicating the currently
///                 active editor modes.
/// * `buffers`: A `Res` holding the `Buffers` resource, used to retrieve information
///              about the current buffer and its cursors.
pub async fn render_statusline(
    chunk: Chunk<StatuslineChunk>,
    plugin_config: Res<PluginConfig>,
    theme: Res<Theme>,
    mode_stack: Res<ModeStack>,

    buffers: Res<Buffers>,
) {
    // Deserialize statusline-specific configuration from the plugin config
    let plugin_config = plugin_config
        .get()
        .0
        .get("statusline")
        .map(|x| StatuslineConfig::deserialize(x.clone()).unwrap())
        .unwrap_or_default();

    let theme = theme.get();
    let mode_stack = mode_stack.get();

    let mut chunk = chunk.get().unwrap();

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
            return;
        }
        // Render each part, adding " -> " separator if it's not the first part
        loc = render!(chunk, loc => [if i != 0 {" -> "} else {""}, theme.apply(text)]);
    }

    // Get information about the current buffer for cursor display
    let cur_buf = buffers.get().cur_buffer();
    let cur_buf = cur_buf.read().unwrap();

    let mut loc = chunk.size() - vec2(1, 0); // Start rendering from the right edge

    // Display cursor count
    let cursor_count = cur_buf.cursors.len().saturating_sub(1);
    if cursor_count == 0 {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.one", "statusline.selections"]);
        // Render "1 sel" for a single cursor
        render!(chunk, loc - vec2(5, 0) => [sel_style.apply("1 sel")]);
    } else {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.multi", "statusline.selections"]);
        let primary_cursor = cur_buf.primary_cursor;
        let render_text = format!("{primary_cursor}/{cursor_count} sels");
        loc.x = loc.x.saturating_sub(render_text.len() as u16);
        // Render "{primary_cursor}/{total_cursors} sels" for multiple cursors
        render!(chunk, loc => [sel_style.apply(render_text)]);
    }
}
