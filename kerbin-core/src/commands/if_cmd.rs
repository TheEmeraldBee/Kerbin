use crate::*;

#[derive(Command)]
pub enum IfCommand {
    #[command]
    /// Executes commands only if the given check passes.
    ///
    /// The `--invert` flag inverts the check result.
    If {
        #[command(type_name = "[check]")]
        check: Vec<Token>,

        #[command(flag, type_name = "[command]")]
        cmds: Option<Vec<Token>>,

        #[command(flag, type_name = "[command]")]
        else_cmds: Option<Vec<Token>>,
    },

    Then {
        #[command(type_name = "[command]")]
        first: Vec<Token>,

        #[command(type_name = "[command]")]
        then: Vec<Token>,
    },
}

async fn run_token_cmds(tokens: &[Token], state: &mut State) -> bool {
    let token_lists: Vec<Vec<Token>> = if tokens.iter().all(|t| matches!(t, Token::List(_))) {
        tokens
            .iter()
            .filter_map(|t| {
                if let Token::List(items) = t {
                    Some(tokenize(&tokens_to_command_string(items)).unwrap_or_default())
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![tokens.to_vec()]
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
            if !dispatch_command(command.as_ref(), state).await {
                return false;
            };
        } else {
            return false;
        }
    }

    true
}

#[async_trait::async_trait]
impl Command<State> for IfCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::If {
                check,
                cmds,
                else_cmds,
            } => {
                let check_tokens: Vec<Token> = if check.len() == 1 {
                    if let Token::List(items) = &check[0] {
                        items.clone()
                    } else {
                        check.clone()
                    }
                } else {
                    check.clone()
                };

                let if_check = state
                    .lock_state::<IfCheckRegistry>()
                    .await
                    .parse(&check_tokens);

                let result = match if_check {
                    Some(c) => c.check(state).await,
                    None => return false,
                };

                if result {
                    if let Some(cmds) = cmds {
                        return run_token_cmds(cmds, state).await;
                    }
                } else if let Some(else_cmds) = else_cmds {
                    return run_token_cmds(else_cmds, state).await;
                }

                true
            }
            Self::Then { first, then } => {
                if !run_token_cmds(first, state).await {
                    return false;
                }
                run_token_cmds(then, state).await
            }
        }
    }
}
