use crate::*;

#[derive(Command)]
pub enum DialogueCommand {
    #[command]
    /// Opens a modal input dialogue with a title, description, and on-submit commands.
    ///
    /// `--title` sets the dialogue title.
    /// `--desc` sets an optional description shown above the input field.
    /// `--input-kind` controls validation: "str" (any text), "cmd" (valid command), "commands" (valid command list).
    /// `--var` names the resolver template variable set to the user's input on submit.
    /// `--commands` is a list of commands to execute on submit (each element is a [command] list).
    /// `--on-change` is an optional list of commands to execute on each input change.
    Dialogue {
        #[command(flag)]
        title: String,
        #[command(flag)]
        desc: Option<String>,
        #[command(flag, name = "input-kind")]
        input_kind: String,
        #[command(flag)]
        var: String,
        #[command(flag, name = "commands", type_name = "[command_list]")]
        commands: Vec<Token>,
        #[command(flag, name = "on-change", type_name = "[command_list]")]
        on_change: Option<Vec<Token>>,
    },

    #[command]
    /// Appends a string to the dialogue input field
    DialoguePush(String),

    #[command]
    /// Removes the last N characters from the dialogue input field
    DialoguePop(usize),

    #[command]
    /// Submits the dialogue input, sets the template variable, and executes on-submit commands
    DialogueSubmit,

    #[command]
    /// Cancels the dialogue without executing any commands
    DialogueCancel,
}

#[async_trait::async_trait]
impl Command<State> for DialogueCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Dialogue {
                title,
                desc,
                input_kind,
                var,
                commands,
                on_change,
            } => {
                let ik = match input_kind.as_str() {
                    "cmd" => InputKind::Cmd,
                    "commands" => InputKind::Commands,
                    _ => InputKind::Str,
                };

                let mut dialogue = state.lock_state::<DialogueState>().await;
                dialogue.active = true;
                dialogue.title = title.clone();
                dialogue.desc = desc.clone().unwrap_or_default();
                dialogue.input_kind = ik;
                dialogue.var_name = var.clone();
                dialogue.commands = commands.clone();
                dialogue.on_change = on_change.clone().unwrap_or_default();
                dialogue.input.clear();
                dialogue.input_valid = true;
                drop(dialogue);

                state.lock_state::<ModeStack>().await.push_mode('d');
                true
            }

            Self::DialoguePush(content) => {
                let (on_change, var_name, input) = {
                    let mut dialogue = state.lock_state::<DialogueState>().await;
                    if !dialogue.active {
                        return false;
                    }
                    dialogue.input.push_str(content.as_str());
                    (
                        dialogue.on_change.clone(),
                        dialogue.var_name.clone(),
                        dialogue.input.clone(),
                    )
                };
                run_dialogue_on_change(state, &on_change, &var_name, &input).await;
                true
            }

            Self::DialoguePop(count) => {
                let (on_change, var_name, input) = {
                    let mut dialogue = state.lock_state::<DialogueState>().await;
                    if !dialogue.active {
                        return false;
                    }
                    for _ in 0..*count {
                        dialogue.input.pop();
                        if dialogue.input.is_empty() {
                            return false;
                        }
                    }
                    (
                        dialogue.on_change.clone(),
                        dialogue.var_name.clone(),
                        dialogue.input.clone(),
                    )
                };
                run_dialogue_on_change(state, &on_change, &var_name, &input).await;
                true
            }

            Self::DialogueSubmit => {
                let (input, var_name, commands, input_kind, is_valid) = {
                    let dialogue = state.lock_state::<DialogueState>().await;
                    (
                        dialogue.input.clone(),
                        dialogue.var_name.clone(),
                        dialogue.commands.clone(),
                        dialogue.input_kind.clone(),
                        dialogue.input_valid,
                    )
                };

                if input.is_empty() {
                    return false;
                }
                if input_kind != InputKind::Str && !is_valid {
                    return false;
                }

                {
                    let mut dialogue = state.lock_state::<DialogueState>().await;
                    dialogue.active = false;
                    dialogue.input.clear();
                }
                state.lock_state::<ModeStack>().await.pop_mode();

                resolver_engine_mut().await.set_template(&var_name, &input);

                for token in &commands {
                    if let Token::List(cmd_tokens) = token {
                        let command = state.lock_state::<CommandRegistry>().await.parse_command(
                            cmd_tokens.clone(),
                            true,
                            false,
                            Some(&resolver_engine().await.as_resolver()),
                            true,
                            &*state.lock_state::<CommandPrefixRegistry>().await,
                            &*state.lock_state::<ModeStack>().await,
                        );
                        if let Some(cmd) = command
                            && let Err(e) = state.lock_state::<CommandSender>().await.send(cmd) {
                                tracing::error!("dialogue: failed to send command: {e}");
                            }
                    }
                }

                resolver_engine_mut().await.remove_template(&var_name);

                true
            }

            Self::DialogueCancel => {
                let var_name = {
                    let mut dialogue = state.lock_state::<DialogueState>().await;
                    if !dialogue.active {
                        return false;
                    }
                    let var_name = dialogue.var_name.clone();
                    dialogue.active = false;
                    dialogue.input.clear();
                    var_name
                };
                state.lock_state::<ModeStack>().await.pop_mode();
                resolver_engine_mut().await.remove_template(&var_name);
                true
            }
        }
    }
}
