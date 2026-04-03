use std::{collections::HashMap, sync::Arc};

use crossterm::event::KeyModifiers;

use crate::{
    Matchable, ParsableKey, ParseError, ResolvedKeyBind, UnresolvedKeyBind, UnresolvedKeyElement,
};
use kerbin_command_lang::*;

pub type CommandExecutor =
    dyn Fn(&str, &[String]) -> Result<Vec<String>, ParseError> + Send + Sync + 'static;

pub struct Resolver<'a> {
    templates: &'a HashMap<String, Token>,
    command_executor: Arc<CommandExecutor>,
}

impl<'a> Resolver<'a> {
    pub fn new(templates: &'a HashMap<String, Token>, executor: Arc<CommandExecutor>) -> Self {
        Self {
            templates,
            command_executor: executor,
        }
    }

    /// Resolve a single UnresolvedKeyBind into all possible ResolvedKeyBind permutations
    pub fn resolve(&self, bind: UnresolvedKeyBind) -> Result<Vec<ResolvedKeyBind>, ParseError> {
        let mut modifier_options: Vec<Vec<Matchable<KeyModifiers>>> = Vec::new();

        for mod_elem in bind.mods {
            let resolved = self.resolve_element(mod_elem)?;
            modifier_options.push(resolved);
        }

        // bind.code is UnresolvedKeyElement<ResolvedKeyBind>
        let code_options = self.resolve_element(bind.code)?;

        let mod_permutations = Self::cartesian_product(&modifier_options);
        let mut results = Vec::new();

        for mod_combo in mod_permutations {
            let combined_mods = mod_combo
                .into_iter()
                .fold(Matchable::Specific(KeyModifiers::empty()), |acc, m| acc | m);

            for code_part in &code_options {
                // Combine outer modifiers with inner modifiers from the key part
                let final_mods = combined_mods | code_part.mods;
                results.push(ResolvedKeyBind {
                    mods: final_mods,
                    code: code_part.code,
                });
            }
        }

        Ok(results)
    }

    /// Expand a list of tokens by resolving variables and command substitutions.
    pub fn expand_tokens(&self, tokens: Vec<Token>, allow_run: bool) -> Vec<Token> {
        self.expand_tokens_reporting(tokens, allow_run, &mut vec![])
    }

    /// Like [`expand_tokens`], but appends human-readable error messages to `errors` for
    /// unknown templates and failed command substitutions instead of silently swallowing them.
    pub fn expand_tokens_reporting(
        &self,
        tokens: Vec<Token>,
        allow_run: bool,
        errors: &mut Vec<String>,
    ) -> Vec<Token> {
        tokens
            .into_iter()
            .flat_map(|t| match t {
                Token::Word(s) => vec![Token::Word(s)],

                Token::Variable(name) => {
                    if let Some(value) = self.templates.get(&name) {
                        let items = match value {
                            Token::List(items) => items.clone(),
                            other => vec![other.clone()],
                        };
                        self.expand_tokens_reporting(items, allow_run, errors)
                    } else {
                        errors.push(format!("Unknown template: %{name}"));
                        vec![Token::Variable(name)]
                    }
                }

                Token::CommandSubst(inner) if allow_run => {
                    tracing::info!(%inner, %allow_run);
                    let cmd_tokens = tokenize(&inner).unwrap_or_default();
                    let expanded = self.expand_tokens_reporting(cmd_tokens, allow_run, errors);
                    let parts: Vec<String> = expanded
                        .into_iter()
                        .filter_map(|t| {
                            if let Token::Word(s) = t {
                                Some(s)
                            } else {
                                None
                            }
                        })
                        .collect();

                    if let Some((cmd, args)) = parts.split_first() {
                        match (self.command_executor)(cmd, args) {
                            Ok(output) => output.into_iter().map(Token::Word).collect(),
                            Err(e) => {
                                errors.push(format!("Command `{inner}` failed: {e}"));
                                vec![]
                            }
                        }
                    } else {
                        vec![]
                    }
                }

                Token::CommandSubst(inner) => vec![Token::CommandSubst(inner)],

                Token::List(inner) => vec![Token::List(
                    self.expand_tokens_reporting(inner, allow_run, errors),
                )],

                Token::Interpolated(parts) => {
                    let expanded = self.expand_tokens_reporting(parts, allow_run, errors);
                    // If any inner token is still unresolved, preserve this Interpolated
                    // so it can be expanded again later when the template is populated.
                    if expanded
                        .iter()
                        .any(|t| matches!(t, Token::Variable(_) | Token::Interpolated(_)))
                    {
                        return vec![Token::Interpolated(expanded)];
                    }
                    let joined = expanded
                        .into_iter()
                        .map(|t| match t {
                            Token::Word(s) => s,
                            Token::Variable(name) => format!("%{}", name),
                            Token::CommandSubst(s) => format!("$({})", s),
                            Token::Interpolated(inner) => flatten_tokens(inner),
                            Token::List(inner) => format!("[{}]", flatten_tokens(inner)),
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    vec![Token::Word(joined)]
                }
            })
            .collect()
    }

    /// Expand a raw string by tokenizing it and resolving variables/substitutions.
    /// Used as a compatibility shim where a flat string result is needed.
    pub fn expand_str(&self, input: &str, allow_run: bool) -> String {
        let tokens = tokenize(input).unwrap_or_default();
        flatten_tokens(self.expand_tokens(tokens, allow_run))
    }

    /// Resolve a single UnresolvedKeyElement into a list of possible values
    fn resolve_element<T: ParsableKey<Output = T>>(
        &self,
        element: UnresolvedKeyElement<T>,
    ) -> Result<Vec<T>, ParseError> {
        match element {
            UnresolvedKeyElement::Literal(value) => Ok(vec![value]),

            UnresolvedKeyElement::OneOf(values) => Ok(values),

            UnresolvedKeyElement::Template(template_name) => {
                let token = self.templates.get(&template_name).ok_or_else(|| {
                    ParseError::Custom(format!("Template '{}' not found", template_name))
                })?;

                let raw_items = match token {
                    Token::List(items) => items.clone(),
                    other => vec![other.clone()],
                };

                let mut errors = Vec::new();
                let expanded = self.expand_tokens_reporting(raw_items, false, &mut errors);

                let mut results = Vec::new();
                for t in &expanded {
                    let s = match t {
                        Token::Word(s) => s.as_str(),
                        other => &token_to_string(other),
                    };
                    results.push(T::parse_from_str(s)?);
                }
                Ok(results)
            }

            UnresolvedKeyElement::Command(cmd, args) => {
                let output = (self.command_executor)(&cmd, &args)?;

                let mut results = Vec::new();
                for value_str in output {
                    results.push(T::parse_from_str(&value_str)?);
                }
                Ok(results)
            }
        }
    }

    /// Generate the cartesian product of multiple vectors
    fn cartesian_product<T: Clone>(lists: &[Vec<T>]) -> Vec<Vec<T>> {
        if lists.is_empty() {
            return vec![vec![]];
        }

        let mut result = vec![vec![]];

        for list in lists {
            let mut new_result = Vec::new();
            for existing in &result {
                for item in list {
                    let mut new_combo = existing.clone();
                    new_combo.push(item.clone());
                    new_result.push(new_combo);
                }
            }
            result = new_result;
        }

        result
    }
}
