use ratatui::prelude::*;

use crate::*;

/// State for storing registered commands and parsing input
#[derive(State)]
pub struct CommandRegistry(pub Vec<RegisteredCommandSet>);

impl CommandRegistry {
    /// Registers a command type within the editor
    pub fn register<T: CommandFromStr<State> + 'static>(&mut self) {
        self.0.push(RegisteredCommandSet {
            parser: Box::new(T::from_str),
            infos: T::infos(),
        })
    }

    /// Determines if the input string represents a valid command
    pub fn validate_command(
        &self,
        input: &str,
        resolver: Option<&Resolver<'_>>,

        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> bool {
        let tokens = tokenize(input).unwrap_or_default();

        // Expand without running — CommandSubst tokens remain if not yet resolvable.
        let expanded = if let Some(r) = resolver {
            r.expand_tokens(tokens.clone(), false)
        } else {
            tokens.clone()
        };

        // If any dynamic tokens remain unresolved we can't statically validate;
        // optimistically treat the input as valid.
        if has_dynamic_tokens(&expanded) {
            return true;
        }

        self.parse_command(tokens, false, true, resolver, false, prefix_registry, modes)
            .is_some()
    }

    /// Retrieves command suggestions and theming for the palette
    pub async fn get_command_suggestions(
        &self,
        input: &str,
        theme: &Theme,
    ) -> (
        Vec<Line<'static>>,
        Option<String>,
        Option<Vec<Line<'static>>>,
    ) {
        let resolver = resolver_engine().await;
        resolver.as_resolver().expand_str(input, false);

        let tokens = tokenize(input).unwrap_or_default();

        if tokens.is_empty() {
            return (vec![], None, None);
        }

        let first_name = match tokens.first() {
            Some(Token::Word(s)) => s.clone(),
            _ => return (vec![], None, None),
        };

        let mut res = vec![];

        for registry in &self.0 {
            for info in &registry.infos {
                for valid_name in &info.valid_names {
                    let Some(rnk) = rank(&first_name, valid_name) else {
                        continue;
                    };

                    res.push((rnk, info, valid_name.to_string()));
                    break;
                }
            }
        }

        res.sort_by(|l, r| l.0.cmp(&r.0));

        let desc = res.first().and_then(|x| x.1.desc_buf(theme));

        let completion = if tokens.len() == 1 {
            res.first().map(|x| x.2.clone())
        } else {
            None
        };

        (
            res.iter()
                .enumerate()
                .map(|(i, x)| {
                    if i == 0 && completion.is_some() {
                        x.1.as_suggestion_with_search(true, input, theme)
                    } else {
                        x.1.as_suggestion_with_search(false, input, theme)
                    }
                })
                .collect(),
            completion,
            desc,
        )
    }

    #[allow(clippy::too_many_arguments)]
    /// Parses a list of tokens into a runnable command
    pub fn parse_command(
        &self,
        mut tokens: Vec<Token>,
        log_errors: bool,
        prefix_checked: bool,

        resolver: Option<&Resolver<'_>>,
        allow_run: bool,

        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> Option<Box<dyn Command<State>>> {
        if let Some(resolver) = resolver {
            tokens = resolver.expand_tokens(tokens, allow_run);
        }

        if !prefix_checked {
            for prefix in &prefix_registry.0 {
                if prefix.modes.iter().any(|x| modes.mode_on_stack(*x)) {
                    let first_word = match tokens.first() {
                        Some(Token::Word(s)) => s.clone(),
                        _ => String::new(),
                    };

                    let mut has_name = false;
                    if !prefix.list.is_empty() {
                        for infos in &self.0 {
                            if infos.infos.iter().any(|x| {
                                let matches_word0 = x.check_name(&first_word);
                                let matches_prefix = prefix.list.iter().any(|l| x.check_name(l));
                                matches_word0 && matches_prefix
                            }) {
                                has_name = true;
                            }

                            if has_name {
                                break;
                            }
                        }
                    } else {
                        has_name = true
                    }

                    if prefix.include != has_name {
                        continue;
                    }

                    let template = tokenize(&prefix.prefix_cmd).unwrap_or_default();
                    tokens = if has_cmd_placeholder(&template) {
                        apply_cmd_template(template, tokens)
                    } else {
                        let mut t = template;
                        t.push(Token::List(tokens));
                        t
                    };
                }
            }
        }

        if tokens.is_empty() {
            return None;
        }

        for registry in &self.0 {
            if let Some(cmd) = (registry.parser)(&tokens) {
                match cmd {
                    Ok(t) => return Some(t),
                    Err(e) => {
                        if log_errors {
                            tracing::error!("Failed to parse command due to: {e:?}");
                        }
                        return None;
                    }
                }
            }
        }
        None
    }
}

fn has_dynamic_tokens(tokens: &[Token]) -> bool {
    tokens.iter().any(|t| match t {
        Token::CommandSubst(_) | Token::Variable(_) => true,
        Token::List(inner) | Token::Interpolated(inner) => has_dynamic_tokens(inner),
        Token::Word(_) => false,
    })
}

/// Returns true if `tokens` contain a `%cmd` placeholder anywhere in the tree.
fn has_cmd_placeholder(tokens: &[Token]) -> bool {
    tokens.iter().any(|t| match t {
        Token::Variable(name) if name == "cmd" => true,
        Token::List(inner) | Token::Interpolated(inner) => has_cmd_placeholder(inner),
        _ => false,
    })
}

/// Recursively substitutes `%cmd` in `template` with the original `cmd_tokens`,
/// spreading them inline so `[%cmd]` becomes `[tok1 tok2 ...]`.
fn apply_cmd_template(template: Vec<Token>, cmd_tokens: Vec<Token>) -> Vec<Token> {
    let mut result = Vec::new();
    for tok in template {
        match tok {
            Token::Variable(ref name) if name == "cmd" => {
                result.extend(cmd_tokens.iter().cloned());
            }
            Token::List(inner) => {
                result.push(Token::List(apply_cmd_template(inner, cmd_tokens.clone())));
            }
            Token::Interpolated(inner) => {
                result.push(Token::Interpolated(apply_cmd_template(
                    inner,
                    cmd_tokens.clone(),
                )));
            }
            other => result.push(other),
        }
    }
    result
}
