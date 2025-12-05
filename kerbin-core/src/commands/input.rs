use crate::*;

#[derive(Command)]
pub enum InputCommand {
    /// Pushes the digits onto the repeat string for repeating input commands
    #[command(name = "p_rep")]
    PushRepeatNumber(char),

    /// Pops the number of digits off of the input commands
    #[command(name = "r_rep")]
    PopRepeatNumber(usize),
}

#[async_trait::async_trait]
impl Command for InputCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut input = state.lock_state::<InputState>().await;

        match self {
            Self::PushRepeatNumber(i) => {
                if !i.is_ascii_digit() {
                    return false;
                }

                if input.repeat_count.is_empty() && *i == '0' {
                    // Disallow pushing '0' as first char
                    return false;
                }

                input.repeat_count.push_str(&i.to_string());
                true
            }

            Self::PopRepeatNumber(count) => {
                for _ in 0..*count {
                    input.repeat_count.pop();
                    if input.repeat_count.is_empty() {
                        return false;
                    }
                }
                true
            }
        }
    }
}
