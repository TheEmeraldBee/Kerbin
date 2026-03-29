use crate::Token;

/// Type alias for a command parsing function generic over state type `S`.
pub type CommandFn<S> = Box<
    dyn Fn(&[Token]) -> Option<Result<Box<dyn Command<S>>, String>> + Send + Sync,
>;

/// Represents a set of registered commands, including its parser and command information.
pub struct RegisteredCommandSet<S: Send + Sync + 'static> {
    pub parser: CommandFn<S>,
    pub infos: Vec<CommandInfo>,
}

/// Represents a command prefix configuration.
#[derive(Debug)]
pub struct CommandPrefix {
    pub modes: Vec<char>,
    pub prefix_cmd: String,
    pub include: bool,
    pub list: Vec<String>,
}

/// Allows downcasting a `Box<dyn Command<S>>` to a concrete type for typed interception.
pub trait CommandAny {
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync);
}

/// A command that applies a change to an application state of type `S`.
#[async_trait::async_trait]
pub trait Command<S: Send + Sync + 'static>: CommandAny + Send + Sync {
    async fn apply(&self, state: &mut S) -> bool;
}

#[derive(Debug)]
pub struct CommandInfo {
    pub valid_names: Vec<String>,
    pub args: Vec<(String, String)>,
    pub desc: Vec<String>,
}

impl CommandInfo {
    pub fn new(
        names: impl IntoIterator<Item = impl ToString>,
        args: impl IntoIterator<Item = (impl ToString, impl ToString)>,
        desc: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        Self {
            valid_names: names.into_iter().map(|x| x.to_string()).collect(),
            args: args
                .into_iter()
                .map(|x| (x.0.to_string(), x.1.to_string()))
                .collect(),
            desc: desc.into_iter().map(|x| x.to_string()).collect(),
        }
    }

    pub fn check_name(&self, name: impl ToString) -> bool {
        self.valid_names.contains(&name.to_string())
    }
}

/// Provides command metadata (names, args, description). Not generic — metadata is
/// independent of the state type.
pub trait AsCommandInfo: CommandAny + Send + Sync {
    fn infos() -> Vec<CommandInfo>
    where
        Self: Sized;
}

/// Allows a command type to be parsed from a token slice for a specific state type `S`.
pub trait CommandFromStr<S: Send + Sync + 'static>: Command<S> + AsCommandInfo {
    fn from_str(val: &[Token]) -> Option<Result<Box<dyn Command<S>>, String>>
    where
        Self: Sized;
}
