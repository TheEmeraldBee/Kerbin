use std::{collections::HashMap, sync::Arc};

use ascii_forge::window::KeyModifiers;

use crate::{
    Matchable, ParsableKey, ParseError, ResolvedKeyBind, Token, UnresolvedKeyBind,
    UnresolvedKeyElement, flatten_tokens, tokenize,
};

pub type CommandExecutor =
    dyn Fn(&str, &[String]) -> Result<Vec<String>, ParseError> + Send + Sync + 'static;

pub struct Resolver<'a> {
    templates: &'a HashMap<String, Vec<String>>,
    command_executor: Arc<CommandExecutor>,
}

impl<'a> Resolver<'a> {
    pub fn new(
        templates: &'a HashMap<String, Vec<String>>,
        executor: Arc<CommandExecutor>,
    ) -> Self {
        Self {
            templates,
            command_executor: executor,
        }
    }

    /// Resolve a single UnresolvedKeyBind into all possible ResolvedKeyBind permutations
    pub fn resolve(&self, bind: UnresolvedKeyBind) -> Result<Vec<ResolvedKeyBind>, ParseError> {
        // First, resolve all modifier elements into their possible values
        let mut modifier_options: Vec<Vec<Matchable<KeyModifiers>>> = Vec::new();

        for mod_elem in bind.mods {
            let resolved = self.resolve_element(mod_elem)?;
            modifier_options.push(resolved);
        }

        // Resolve the key code element
        // bind.code is UnresolvedKeyElement<ResolvedKeyBind>
        let code_options = self.resolve_element(bind.code)?;

        // Generate all permutations of modifiers
        let mod_permutations = Self::cartesian_product(&modifier_options);

        // Combine each modifier permutation with each key code
        let mut results = Vec::new();

        for mod_combo in mod_permutations {
            // Combine all modifiers
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
        tokens
            .into_iter()
            .flat_map(|t| match t {
                Token::Word(s) => vec![Token::Word(s)],

                Token::Variable(name) => {
                    if let Some(values) = self.templates.get(&name) {
                        values.iter().map(|v| Token::Word(v.clone())).collect()
                    } else {
                        // Unknown variable: keep as-is
                        vec![Token::Variable(name)]
                    }
                }

                Token::CommandSubst(inner) if allow_run => {
                    tracing::info!(%inner, %allow_run);
                    // Tokenize and expand the inner command string, then execute
                    let cmd_tokens = tokenize(&inner).unwrap_or_default();
                    let expanded = self.expand_tokens(cmd_tokens, allow_run);
                    let parts: Vec<String> = expanded
                        .into_iter()
                        .filter_map(|t| {
                            if let Token::Word(s) = t { Some(s) } else { None }
                        })
                        .collect();

                    if let Some((cmd, args)) = parts.split_first() {
                        match (self.command_executor)(cmd, args) {
                            Ok(output) => output.into_iter().map(Token::Word).collect(),
                            Err(_) => vec![Token::CommandSubst(inner)],
                        }
                    } else {
                        vec![]
                    }
                }

                Token::CommandSubst(inner) => vec![Token::CommandSubst(inner)],

                Token::List(inner) => vec![Token::List(self.expand_tokens(inner, allow_run))],
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
                let template_values = self.templates.get(&template_name).ok_or_else(|| {
                    ParseError::Custom(format!("Template '{}' not found", template_name))
                })?;

                let mut results = Vec::new();
                for value_str in template_values {
                    results.push(T::parse_from_str(value_str)?);
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
