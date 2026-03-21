use std::collections::HashMap;

use crate::*;

/// Trait for a check that can be evaluated against the editor state.
#[async_trait::async_trait]
pub trait IfCheck: Send + Sync {
    async fn check(&self, state: &mut State) -> bool;
}

/// Checks whether a given mode is present on the mode stack.
pub struct ModeExistsCheck(pub char);

#[async_trait::async_trait]
impl IfCheck for ModeExistsCheck {
    async fn check(&self, state: &mut State) -> bool {
        state.lock_state::<ModeStack>().await.mode_on_stack(self.0)
    }
}

/// Checks whether a named template exists in the resolver engine.
pub struct TemplateExistsCheck(pub String);

#[async_trait::async_trait]
impl IfCheck for TemplateExistsCheck {
    async fn check(&self, _state: &mut State) -> bool {
        resolver_engine().await.has_template(&self.0)
    }
}

/// Checks whether a text string is non-empty.
pub struct TextExistsCheck(pub String);

#[async_trait::async_trait]
impl IfCheck for TextExistsCheck {
    async fn check(&self, _state: &mut State) -> bool {
        !self.0.is_empty()
    }
}

type IfCheckFn = Box<dyn Fn(&[Token]) -> Option<Box<dyn IfCheck>> + Send + Sync>;

/// Registry of named check factories for the `if` command.
#[derive(State, Default)]
pub struct IfCheckRegistry {
    checks: HashMap<String, IfCheckFn>,
}

impl IfCheckRegistry {
    /// Registers a check factory under the given name.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&[Token]) -> Option<Box<dyn IfCheck>> + Send + Sync + 'static,
    ) {
        self.checks.insert(name.into(), Box::new(f));
    }

    /// Parses tokens into a check: first token is the check name, remaining are args.
    pub fn parse(&self, tokens: &[Token]) -> Option<Box<dyn IfCheck>> {
        let name = match tokens.first() {
            Some(Token::Word(w)) => w.as_str(),
            _ => return None,
        };
        let factory = self.checks.get(name)?;
        factory(&tokens[1..])
    }
}
