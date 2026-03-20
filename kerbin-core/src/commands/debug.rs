use crate::*;

#[derive(Command)]
pub enum DebugCommand {
    #[command]
    /// Outputs text/templates as raw text to the screen.
    ///
    /// `--level` can be: `low`|`medium`|`high`|`critical`
    /// Defaults to `medium`
    Echo {
        text: Vec<String>,

        #[command(flag)]
        level: Option<String>,
    },

    #[command]
    /// Executes the given commands, only if the condition string is not empty
    ///
    /// The `--invert` flag makes it check if it's empty
    If {
        #[command(type_name = "[string]")]
        cond: Vec<String>,

        #[command(flag)]
        invert: bool,

        #[command(flag, type_name = "[command]")]
        cmds: Vec<Token>,
    },
}

#[async_trait::async_trait]
impl Command for DebugCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Echo { text, level } => {
                let text = text.join(" ");
                let log = state.lock_state::<LogSender>().await;
                if let Some(level) = level {
                    let _ = match level.as_str() {
                        "low" => log.low("echo", text),
                        "medium" => log.medium("echo", text),
                        "high" => log.high("echo", text),
                        "critical" => log.critical("echo", text),
                        _ => log.medium("echo", text),
                    };
                } else {
                    log.medium("echo", text);
                }

                true
            }
            Self::If { cond, invert, cmds } => {
                let cond = cond.join(" ");
                if cond != "" && *invert {
                    return false;
                } else if cond == "" && !*invert {
                    return false;
                }

                let token_lists: Vec<Vec<Token>> =
                    if cmds.iter().all(|t| matches!(t, Token::List(_))) {
                        cmds.iter()
                            .filter_map(|t| {
                                if let Token::List(items) = t {
                                    Some(
                                        tokenize(&tokens_to_command_string(&items))
                                            .unwrap_or_default(),
                                    )
                                } else {
                                    None
                                }
                            })
                            .collect()
                    } else {
                        vec![cmds.clone()]
                    };

                for token_list in token_lists {
                    let command = state.lock_state::<CommandRegistry>().await.parse_command(
                        token_list,
                        true,
                        false,
                        Some(&resolver_engine().await.as_resolver()),
                        true,
                        &*state.lock_state::<CommandPrefixRegistry>().await,
                        &*state.lock_state::<ModeStack>().await,
                    );
                    if let Some(command) = command {
                        state
                            .lock_state::<CommandSender>()
                            .await
                            .send(command)
                            .unwrap();
                    }
                }

                true
            }
        }
    }
}
