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

    let mut loc = window.size() - vec2(0, 3);

    for (i, (text, theme)) in parts.into_iter().enumerate() {
        loc = render!(window, loc => [if i != 0 {" -> "} else {""}, theme.apply(text)]);
    }
}
