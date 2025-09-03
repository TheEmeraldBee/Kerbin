use std::collections::HashMap;

use crate::*;
use ascii_forge::prelude::*;
use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};
use serde::Deserialize;

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct ModeConfig {
    pub long_name: Option<String>,

    /// The key from the theme for the plugin data
    pub theme_key: Option<String>,

    pub hide_others: bool,
}

#[derive(Deserialize, Default, Debug)]
pub struct StatuslineConfig {
    pub modes: HashMap<char, ModeConfig>,
}

pub async fn render_statusline(
    chunk: Chunk<StatuslineChunk>,
    plugin_config: Res<PluginConfig>,
    theme: Res<Theme>,
    mode_stack: Res<ModeStack>,

    buffers: Res<Buffers>,
) {
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

    for part in &mode_stack.0 {
        if let Some(config) = plugin_config.modes.get(part) {
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

    for (i, (text, theme)) in parts.into_iter().enumerate().take(4) {
        if loc.x >= chunk.size().x {
            return;
        }
        loc = render!(chunk, loc => [if i != 0 {" -> "} else {""}, theme.apply(text)]);
    }

    let cur_buf = buffers.get().cur_buffer();
    let cur_buf = cur_buf.read().unwrap();

    let mut loc = chunk.size() - vec2(1, 0);

    let cursor_count = cur_buf.cursors.len().saturating_sub(1);
    if cursor_count == 0 {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.one", "statusline.selections"]);
        render!(chunk, loc - vec2(5, 0) => [sel_style.apply("1 sel")]);
    } else {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.multi", "statusline.selections"]);
        let primary_cursor = cur_buf.primary_cursor;
        let render_text = format!("{primary_cursor}/{cursor_count} sels");
        loc.x = loc.x.saturating_sub(render_text.len() as u16);
        render!(chunk, loc => [sel_style.apply(render_text)]);
    }
}
