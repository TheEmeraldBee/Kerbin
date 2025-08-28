use std::{collections::HashMap, sync::Arc};

use crate::*;
use ascii_forge::prelude::*;
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

pub fn render_statusline(state: Arc<State>) {
    let plugin_config = state
        .plugin_config
        .read()
        .unwrap()
        .get("statusline")
        .map(|x| StatuslineConfig::deserialize(x.clone()).unwrap())
        .unwrap_or_default();

    let theme = state.theme.read().unwrap();
    let mode_stack = state.mode_stack.read().unwrap();

    let mut window = state.window.write().unwrap();

    let mut parts = vec![];

    for part in mode_stack.iter() {
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

    let mut loc = window.size() - vec2(0, 2);
    loc.x = 0;

    for (i, (text, theme)) in parts.into_iter().enumerate().take(4) {
        if loc.x >= window.size().x {
            return;
        }
        loc = render!(window, loc => [if i != 0 {" -> "} else {""}, theme.apply(text)]);
    }

    let cur_buf = state.buffers.read().unwrap().cur_buffer();
    let cur_buf = cur_buf.read().unwrap();

    let mut loc = window.size() - vec2(1, 2);

    let cursor_count = cur_buf.cursors.len().saturating_sub(1);
    if cursor_count == 0 {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.one", "statusline.selections"]);
        render!(window, loc - vec2(5, 0) => [sel_style.apply("1 sel")]);
    } else {
        let sel_style =
            theme.get_fallback_default(["statusline.selections.multi", "statusline.selections"]);
        let primary_cursor = cur_buf.primary_cursor;
        let render_text = format!("{primary_cursor}/{cursor_count} sels");
        loc.x = loc.x.saturating_sub(render_text.len() as u16);
        render!(window, loc => [sel_style.apply(render_text)]);
    }
}
