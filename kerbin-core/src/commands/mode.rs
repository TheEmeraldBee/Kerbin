use crate::*;

#[derive(Debug, Clone, Command)]
pub enum ModeCommand {
    #[command(name = "cm")]
    /// Clears the mode stack and sets it to the given char
    /// Should almost never be used, please use "pm" and "rm" instead
    ChangeMode(char),

    #[command(name = "pm")]
    /// Pushes a new mode to the mode stack
    PushMode(char),

    #[command(name = "rm")]
    /// Pops the current mode from the mode stack, limited at the base ('n')
    PopMode,
}

#[async_trait::async_trait]
impl Command for ModeCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut modes = state.lock_state::<ModeStack>().await;

        match *self {
            ModeCommand::ChangeMode(new) => modes.set_mode(new),
            ModeCommand::PushMode(new) => modes.push_mode(new),
            ModeCommand::PopMode => {
                modes.pop_mode();
            }
        }

        // Always return false, as this should never be repeated
        false
    }
}
