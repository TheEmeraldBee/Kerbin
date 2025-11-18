use std::collections::HashMap;

use ascii_forge::window::KeyModifiers;

use crate::{ParsableKey, ParseError, ResolvedKeyBind, UnresolvedKeyBind, UnresolvedKeyElement};

pub type CommandExecutor =
    dyn Fn(&str, &[String]) -> Result<Vec<String>, ParseError> + Send + Sync + 'static;

pub struct Resolver<'a> {
    templates: &'a HashMap<String, Vec<String>>,
    command_executor: &'a CommandExecutor,
}

impl<'a> Resolver<'a> {
    pub fn new(templates: &'a HashMap<String, Vec<String>>, executor: &'a CommandExecutor) -> Self {
        Self {
            templates,
            command_executor: executor,
        }
    }

    /// Resolve a single UnresolvedKeyBind into all possible ResolvedKeyBind permutations
    pub fn resolve(&self, bind: UnresolvedKeyBind) -> Result<Vec<ResolvedKeyBind>, ParseError> {
        // First, resolve all modifier elements into their possible values
        let mut modifier_options: Vec<Vec<KeyModifiers>> = Vec::new();

        for mod_elem in bind.mods {
            let resolved = self.resolve_element(mod_elem)?;
            modifier_options.push(resolved);
        }

        // Resolve the key code element
        let code_options = self.resolve_element(bind.code)?;

        // Generate all permutations of modifiers
        let mod_permutations = Self::cartesian_product(&modifier_options);

        // Combine each modifier permutation with each key code
        let mut results = Vec::new();

        for mod_combo in mod_permutations {
            // Combine all modifiers using bitwise OR
            let combined_mods = mod_combo
                .into_iter()
                .fold(KeyModifiers::empty(), |acc, m| acc | m);

            for code in &code_options {
                results.push(ResolvedKeyBind {
                    mods: combined_mods,
                    code: *code,
                });
            }
        }

        Ok(results)
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

#[cfg(test)]
mod tests {
    use ascii_forge::window::KeyCode;

    use crate::UnresolvedKeyElement;

    use super::*;

    #[test]
    fn test_resolve_simple_literal() {
        let templates = HashMap::new();
        let executor = |_: &str, _: &[String]| -> Result<Vec<String>, ParseError> { Ok(vec![]) };

        let resolver = Resolver::new(&templates, &executor);

        let bind = UnresolvedKeyBind {
            mods: vec![UnresolvedKeyElement::Literal(KeyModifiers::CONTROL)],
            code: UnresolvedKeyElement::Literal(KeyCode::Char('a')),
        };

        let results = resolver.resolve(bind).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_resolve_one_of() {
        let templates = HashMap::new();
        let executor = |_: &str, _: &[String]| -> Result<Vec<String>, ParseError> { Ok(vec![]) };

        let resolver = Resolver::new(&templates, &executor);

        let bind = UnresolvedKeyBind {
            mods: vec![UnresolvedKeyElement::OneOf(vec![
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
            ])],
            code: UnresolvedKeyElement::OneOf(vec![KeyCode::Char('a'), KeyCode::Char('b')]),
        };

        let results = resolver.resolve(bind).unwrap();
        assert_eq!(results.len(), 4); // 2 mods * 2 codes = 4 permutations
    }
}
