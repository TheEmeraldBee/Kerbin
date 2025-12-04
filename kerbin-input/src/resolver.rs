use std::{collections::HashMap, sync::Arc};

use ascii_forge::window::KeyModifiers;

use crate::{
    Matchable, ParsableKey, ParseError, ResolvedKeyBind, UnresolvedKeyBind, UnresolvedKeyElement,
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

    pub fn expand_str(&self, input: &str, allow_run: bool) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\\' => {
                    if let Some(&next) = chars.peek() {
                        if next == '$' {
                            chars.next();
                            result.push('$');
                        } else {
                            result.push('\\');
                        }
                    } else {
                        result.push('\\');
                    }
                }
                '%' => {
                    if chars.peek() == Some(&'%') {
                        chars.next();
                        result.push('%');
                    } else {
                        let template_name: String = chars
                            .by_ref()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();

                        if !template_name.is_empty() {
                            if let Some(values) = self.templates.get(&template_name) {
                                if values.len() == 1 {
                                    result.push_str(&values[0]);
                                } else {
                                    result.push_str(&values.join(" "));
                                }
                            } else {
                                result.push('%');
                                result.push_str(&template_name);
                            }
                        } else {
                            result.push('%');
                        }
                    }
                }
                '$' if chars.peek() == Some(&'(') => {
                    chars.next();

                    let mut cmd_string = String::new();
                    let mut depth = 1;

                    for ch in chars.by_ref() {
                        if ch == '(' {
                            depth += 1;
                            cmd_string.push(ch);
                        } else if ch == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            cmd_string.push(ch);
                        } else {
                            cmd_string.push(ch);
                        }
                    }

                    if allow_run {
                        tracing::info!(%cmd_string, %allow_run);

                        // Execute command regardless of whether closing paren was found
                        let cmd_parts =
                            shellwords::split(&cmd_string).unwrap_or(vec![cmd_string.clone()]);

                        if let Some((cmd, args)) = cmd_parts.split_first() {
                            if let Ok(output) = (self.command_executor)(cmd, args) {
                                if output.len() == 1 {
                                    result.push_str(&output[0]);
                                } else {
                                    result.push_str(&output.join(" "));
                                }
                            } else {
                                result.push_str("$(");
                                result.push_str(&cmd_string);
                                if depth == 0 {
                                    result.push(')');
                                }
                            }
                        } else {
                            result.push_str("$()");
                        }
                    } else {
                        result.push_str("FAIL");
                    }
                }
                _ => {
                    result.push(ch);
                }
            }
        }

        result
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
