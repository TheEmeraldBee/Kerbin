use crate::*;

#[derive(Debug, Clone, Command)]
pub enum ModeCommand {
    #[command(name = "cm")]
    /// Clears the mode stack and sets it to the given char.
    ///
    /// Prefer `pm`/`rm` for normal modal transitions.
    ChangeMode(char),

    #[command(name = "pm")]
    /// Pushes a new mode to the mode stack
    PushMode(char),

    #[command(name = "rm")]
    /// Pops the current mode from the mode stack, limited at the base ('n')
    PopMode,
}

#[async_trait::async_trait]
impl Command<State> for ModeCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut modes = state.lock_state::<ModeStack>().await;

        match *self {
            ModeCommand::ChangeMode(new) => modes.set_mode(new),
            ModeCommand::PushMode(new) => {
                if modes.mode_on_stack(new) {
                    return false;
                }
                modes.push_mode(new);
            }
            ModeCommand::PopMode => {
                modes.pop_mode();
            }
        }

        true
    }
}
