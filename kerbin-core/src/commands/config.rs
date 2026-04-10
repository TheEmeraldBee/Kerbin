use crate::*;
use kerbin_macros::Command;
use kerbin_state_machine::State;

fn parse_key_tokens(keys: &[Token]) -> Vec<UnresolvedKeyBind> {
    keys.iter()
        .filter_map(|t| match t {
            Token::Word(s) => s.parse().ok(),
            Token::Variable(name) => format!("%{}", name).parse().ok(),
            _ => None,
        })
        .collect()
}

fn tokens_to_mode_chars(tokens: &Option<Vec<Token>>) -> Vec<char> {
    tokens
        .as_ref()
        .map(|ts| {
            ts.iter()
                .filter_map(|t| {
                    if let Token::Word(s) = t {
                        s.chars().next()
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn tokens_to_strings(tokens: &[Token]) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|t| {
            if let Token::Word(s) = t {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect()
}

#[derive(Debug, Clone, Command)]
pub enum ConfigCommand {
    /// Bind a key sequence to one or more commands.
    #[command(drop_ident, name = "bind")]
    Bind {
        keys: Vec<Token>,
        #[command(ignore)]
        cmds: Vec<Token>,
        #[command(flag)]
        modes: Option<Vec<Token>>,
        #[command(flag)]
        invalid: Option<Vec<Token>>,
        #[command(flag)]
        required: Option<Vec<String>>,
        #[command(flag)]
        deny_repeat: bool,
        #[command(flag)]
        desc: Option<String>,
    },

    /// Set metadata (description) on a key prefix without binding a command.
    #[command(drop_ident, name = "category")]
    Category {
        keys: Vec<Token>,
        #[command(flag)]
        modes: Option<Vec<Token>>,
        #[command(flag)]
        invalid: Option<Vec<Token>>,
        #[command(flag)]
        desc: Option<String>,
    },

    /// Register a template expansion.
    #[command]
    Template { name: String, value: Token },

    /// List all available templates to the log.
    ///
    /// If `--contains` is passed, only templates whose name contains one of the given substrings are listed.
    #[command]
    ListTemplates(#[command(flag, name = "contains", type_name = "[string]?")] Option<Vec<String>>),

    /// Register a named palette color.
    #[command]
    Palette { name: String, value: String },

    /// Register a theme style entry.
    #[command(drop_ident, name = "theme")]
    Theme {
        key: String,
        #[command(flag)]
        fg: Option<String>,
        #[command(flag)]
        bg: Option<String>,
        #[command(flag)]
        underline: Option<String>,
        #[command(flag)]
        attrs: Option<Vec<String>>,
        value: Option<String>,
    },

    /// Register a command prefix for a set of modes.
    #[command(drop_ident, name = "prefix")]
    Prefix {
        cmd: String,
        #[command(flag)]
        modes: Vec<Token>,
        #[command(flag)]
        include: Option<Vec<Token>>,
        #[command(flag)]
        exclude: Option<Vec<Token>>,
    },

    /// Set a core editor setting (e.g. `core framerate 60`).
    #[command(drop_ident, name = "core")]
    Core { key: String, value: String },

    /// Register a debounce event that fires after idle time in specific modes.
    #[command(drop_ident, name = "debounce_event")]
    DebounceEvent {
        events: Vec<Token>,
        #[command(flag)]
        min_ms: u64,
        #[command(flag)]
        modes: Option<Vec<Token>>,
        #[command(flag)]
        ignore_modes: Option<Vec<Token>>,
        #[command(flag)]
        ignore_with_template: Option<Vec<Token>>,
    },

    /// Configure the statusline display for a specific mode.
    #[command(drop_ident, name = "statusline")]
    Statusline {
        mode: String,
        #[command(flag)]
        long_name: Option<String>,
        #[command(flag)]
        theme_key: Option<String>,
    },

    /// Source another .kb file relative to the current config directory.
    #[command(drop_ident, name = "source")]
    Source { path: String },

    /// Show all config errors from the last load or reload.
    #[command(drop_ident, name = "config_errors")]
    ShowConfigErrors,

    /// Reload runtime config (.kb files) without restarting the editor.
    #[command(drop_ident, name = "reload_config")]
    ReloadConfig,

    /// Bind a command to a mouse event.
    /// Valid event names: left-down, left-up, right-down, right-up, middle, scroll-up, scroll-down
    #[command(drop_ident, name = "mouse_bind")]
    MouseBind { event: String, #[command(ignore)] cmds: Vec<Token> },
}

#[async_trait::async_trait]
impl Command<State> for ConfigCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            ConfigCommand::Bind {
                keys,
                cmds,
                modes,
                invalid,
                required,
                deny_repeat,
                desc,
            } => {
                let key_binds = parse_key_tokens(keys);
                let mode_chars = tokens_to_mode_chars(modes);
                let invalid_chars = tokens_to_mode_chars(invalid);
                let required_tpls = required.clone().unwrap_or_default();

                let commands: Vec<String> = if cmds.iter().all(|t| matches!(t, Token::List(_))) {
                    cmds.iter()
                        .filter_map(|t| {
                            if let Token::List(items) = t {
                                Some(tokens_to_command_string(items))
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    vec![tokens_to_command_string(cmds)]
                };

                let metadata = Metadata {
                    modes: mode_chars,
                    invalid_modes: invalid_chars,
                    required_templates: required_tpls,
                    deny_repeat: *deny_repeat,
                    desc: desc.clone().unwrap_or_default(),
                };

                let resolver_engine = resolver_engine().await;
                let resolver = resolver_engine.as_resolver();
                let mut inputs = state.lock_state::<InputState>().await;
                if let Err(e) = inputs
                    .tree
                    .register(&resolver, key_binds, commands, Some(metadata))
                {
                    tracing::error!("bind: failed to register keybind: {:?}", e);
                }
            }

            ConfigCommand::Category {
                keys,
                modes,
                invalid,
                desc,
            } => {
                let key_binds = parse_key_tokens(keys);
                let metadata = Metadata {
                    modes: tokens_to_mode_chars(modes),
                    invalid_modes: tokens_to_mode_chars(invalid),
                    desc: desc.clone().unwrap_or_default(),
                    ..Metadata::default()
                };
                let resolver_engine = resolver_engine().await;
                let resolver = resolver_engine.as_resolver();
                let mut inputs = state.lock_state::<InputState>().await;
                if let Err(e) = inputs.tree.set_metadata(&resolver, key_binds, metadata) {
                    tracing::error!("category: failed to set metadata: {:?}", e);
                }
            }

            ConfigCommand::Template { name, value } => {
                let items = match value {
                    Token::List(items) => items.clone(),
                    other => vec![other.clone()],
                };

                let resolver_engine = resolver_engine().await;
                let resolver = resolver_engine.as_resolver();
                let expanded = resolver.expand_tokens(items, true);
                drop(resolver_engine);

                let token = match expanded.len() {
                    1 => expanded.into_iter().next().unwrap(),
                    _ => Token::List(expanded),
                };

                resolver_engine_mut().await.set_template(name, token);
            }

            ConfigCommand::ListTemplates(filter) => {
                let filter = filter.clone().unwrap_or_default();

                let log = state.lock_state::<LogSender>().await;

                if filter.is_empty() {
                    log.high(
                        "list-templates",
                        resolver_engine()
                            .await
                            .templates()
                            .iter()
                            .map(|x| x.0.clone())
                            .reduce(|l, r| format!("{l}, {r}"))
                            .unwrap_or_default(),
                    );
                } else {
                    let mut keys = resolver_engine()
                        .await
                        .templates()
                        .keys()
                        .filter(|&x| filter.iter().any(|f| x.contains(f)))
                        .cloned()
                        .collect::<Vec<_>>();

                    keys.sort();

                    let out = keys.join(", ");

                    log.high("list-templates", out);
                }
            }

            ConfigCommand::Palette { name, value } => {
                let palette = state.lock_state::<PaletteState>().await;
                let resolved_palette = palette.0.clone();
                drop(palette);

                if let Some(color) = resolve_color(value, &resolved_palette) {
                    state
                        .lock_state::<PaletteState>()
                        .await
                        .0
                        .insert(name.clone(), color);
                } else {
                    tracing::error!("palette: unknown color '{}' for '{}'", value, name);
                }
            }

            ConfigCommand::Theme {
                key,
                fg,
                bg,
                underline,
                attrs,
                value,
            } => {
                let palette = state.lock_state::<PaletteState>().await.0.clone();

                // Simple form: `theme key colorname` → foreground only
                // Full form: `theme key --fg c --bg c --attrs [list]`
                let (eff_fg, eff_bg, eff_ul, eff_attrs) =
                    if fg.is_none() && bg.is_none() && underline.is_none() && attrs.is_none() {
                        (value.as_deref(), None::<&str>, None::<&str>, vec![])
                    } else {
                        (
                            fg.as_deref(),
                            bg.as_deref(),
                            underline.as_deref(),
                            attrs.clone().unwrap_or_default(),
                        )
                    };

                let style = build_style(eff_fg, eff_bg, eff_ul, &eff_attrs, &palette);
                state
                    .lock_state::<Theme>()
                    .await
                    .register(key.clone(), style);
            }

            ConfigCommand::Prefix {
                cmd,
                modes,
                include,
                exclude,
            } => {
                let mode_chars: Vec<char> = modes
                    .iter()
                    .filter_map(|t| {
                        if let Token::Word(s) = t {
                            s.chars().next()
                        } else {
                            None
                        }
                    })
                    .collect();

                let (include_bool, list) = if let Some(inc) = include {
                    (true, tokens_to_strings(inc))
                } else if let Some(exc) = exclude {
                    (false, tokens_to_strings(exc))
                } else {
                    (false, vec![])
                };

                state
                    .lock_state::<CommandPrefixRegistry>()
                    .await
                    .register(CommandPrefix {
                        modes: mode_chars,
                        prefix_cmd: cmd.clone(),
                        include: include_bool,
                        list,
                    });
            }

            ConfigCommand::Core { key, value } => match key.as_str() {
                "framerate" => {
                    if let Ok(n) = value.parse::<u64>() {
                        state.lock_state::<CoreConfig>().await.framerate = n;
                    }
                }
                "auto_pairs" => match value.as_str() {
                    "enable" => {
                        state.lock_state::<CoreConfig>().await.disable_auto_pairs = false;
                        // Remove any existing registration to avoid duplicates, then re-add.
                        let mut registry = state.lock_state::<CommandInterceptorRegistry>().await;
                        registry.remove_command_interceptor::<BufferCommand>("core::auto_pairs");
                        registry.on_command_named::<BufferCommand>(
                            "core::auto_pairs",
                            0,
                            |cmd, state| Box::pin(auto_pairs_intercept(cmd, state)),
                        );
                    }
                    "disable" => {
                        state.lock_state::<CoreConfig>().await.disable_auto_pairs = true;
                        state
                            .lock_state::<CommandInterceptorRegistry>()
                            .await
                            .remove_command_interceptor::<BufferCommand>("core::auto_pairs");
                    }
                    _ => {
                        state.lock_state::<LogSender>().await.critical(
                            "commands::core",
                            format!("Expected `enable` or `disable`, found: {}", value),
                        );
                    }
                },
                "tab_display_unit" => {
                    state.lock_state::<CoreConfig>().await.tab_display_unit = value.to_string();
                }
                "default_tab_unit" => {
                    if let Ok(n) = value.parse::<usize>() {
                        state.lock_state::<CoreConfig>().await.default_tab_unit = n;
                    }
                }
                "unique_split_buffers" => match value.as_str() {
                    "enable" => {
                        state.lock_state::<SplitState>().await.unique_buffers = true;
                    }
                    "disable" => {
                        state.lock_state::<SplitState>().await.unique_buffers = false;
                    }
                    _ => {
                        state.lock_state::<LogSender>().await.critical(
                            "commands::core",
                            format!("Expected `enable` or `disable`, found: {}", value),
                        );
                    }
                },
                _ => {
                    state.lock_state::<LogSender>().await.critical(
                        "commands::core",
                        format!("Unknown key for core config: {}", key),
                    );
                }
            },

            ConfigCommand::DebounceEvent {
                events,
                min_ms,
                modes,
                ignore_modes,
                ignore_with_template,
            } => {
                let event = crate::debounce::DebounceEvent {
                    events: tokens_to_strings(events),
                    min_ms: *min_ms,
                    modes: tokens_to_mode_chars(modes),
                    ignore_modes: tokens_to_mode_chars(ignore_modes),
                    ignore_with_template: tokens_to_strings(
                        ignore_with_template.as_deref().unwrap_or(&[]),
                    ),
                };
                state.lock_state::<DebounceConfig>().await.0.push(event);
            }

            ConfigCommand::Statusline {
                mode,
                long_name,
                theme_key,
            } => {
                if let Some(mode_char) = mode.chars().next() {
                    let config = ModeConfig {
                        long_name: long_name.clone(),
                        theme_key: theme_key.clone(),
                    };
                    state
                        .lock_state::<StatuslineConfig>()
                        .await
                        .modes
                        .insert(mode_char, config);
                }
            }

            ConfigCommand::Source { path } => {
                let config_dir = state.lock_state::<ConfigDir>().await.0.clone();
                let resolved = config_dir.join(path);
                drop(config_dir);
                let errors = crate::load_kb(&resolved, state).await;
                state.lock_state::<ConfigErrors>().await.0.extend(errors);
            }

            ConfigCommand::ShowConfigErrors => {
                let errors = state.lock_state::<ConfigErrors>().await.0.clone();
                let log = state.lock_state::<LogSender>().await;
                if errors.is_empty() {
                    log.high("config_errors", "No config errors");
                } else {
                    for err in &errors {
                        let msg = if err.line.is_empty() {
                            format!("{}: {}", err.path.display(), err.message)
                        } else {
                            format!("{}: {:?}: {}", err.path.display(), err.line, err.message)
                        };
                        log.high("config_errors", msg);
                    }
                }
            }

            ConfigCommand::MouseBind { event, cmds } => {
                let trigger = match event.as_str() {
                    "left-down" => MouseTrigger::LeftDown,
                    "left-up" => MouseTrigger::LeftUp,
                    "right-down" => MouseTrigger::RightDown,
                    "right-up" => MouseTrigger::RightUp,
                    "middle" => MouseTrigger::MiddleDown,
                    "scroll-up" => MouseTrigger::ScrollUp,
                    "scroll-down" => MouseTrigger::ScrollDown,
                    other => {
                        tracing::error!("mouse_bind: unknown event '{}'", other);
                        return false;
                    }
                };
                let commands = if cmds.iter().all(|t| matches!(t, Token::List(_))) {
                    cmds.iter()
                        .filter_map(|t| {
                            if let Token::List(items) = t {
                                Some(tokens_to_command_string(items))
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    vec![tokens_to_command_string(cmds)]
                };
                state
                    .lock_state::<MouseBindings>()
                    .await
                    .bindings
                    .insert(trigger, commands);
            }

            ConfigCommand::ReloadConfig => {
                let config_path = state.lock_state::<ConfigFolder>().await.0.clone();
                let kb_path = std::path::PathBuf::from(format!("{config_path}/init.kb"));

                crate::reset_config_state(state).await;

                let errors = crate::load_kb(&kb_path, state).await;

                // Mirror the startup auto_pairs default logic (auto_pairs is on by default).
                let disable_auto_pairs = state.lock_state::<CoreConfig>().await.disable_auto_pairs;
                if !disable_auto_pairs {
                    let mut registry = state.lock_state::<CommandInterceptorRegistry>().await;
                    registry.remove_command_interceptor::<BufferCommand>("core::auto_pairs");
                    registry.on_command_named::<BufferCommand>(
                        "core::auto_pairs",
                        0,
                        |cmd, state| Box::pin(auto_pairs_intercept(cmd, state)),
                    );
                }

                *state.lock_state::<ConfigErrors>().await = ConfigErrors(errors.clone());
                let log = state.lock_state::<LogSender>().await;
                if errors.is_empty() {
                    log.low("config", "Config reloaded successfully");
                } else {
                    log.critical(
                        "config",
                        format!(
                            "Config reloaded with {} error(s) — run `config_errors` to review",
                            errors.len()
                        ),
                    );
                }
            }
        }
        false
    }
}
