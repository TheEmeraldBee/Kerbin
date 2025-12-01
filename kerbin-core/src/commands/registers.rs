use crate::*;

#[derive(Command)]
pub enum RegisterCommand {
    #[command(name = "copy")]
    /// Copy the text inside your selection into a register
    ///
    /// Defaults to the 'a' register
    CopyRegister(#[command(type_name = "char?", name = "register")] Option<char>),

    #[command(name = "paste")]
    /// Paste the text inside your register into the editor using a 'a' command
    ///
    /// Defaults to the 'a' register
    ///
    /// Extend defines if selection should extend to the text
    /// Otherwise it just replaces current selection
    ///
    /// Extend defaults to false
    PasteRegister(
        #[command(type_name = "char?", name = "register")] Option<char>,
        #[command(type_name = "bool?", name = "extend")] Option<bool>,
    ),
}

#[async_trait::async_trait]
impl Command for RegisterCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut registers = state.lock_state::<Registers>().await;

        match self {
            Self::CopyRegister(register) => {
                let buf = state.lock_state::<Buffers>().await.cur_buffer().await;

                let byte_range = buf.primary_cursor().sel().clone();
                let text = buf.slice_to_string(*byte_range.start(), *byte_range.end()).unwrap_or_default();

                registers.set(register.unwrap_or('a'), text);

                true
            }
            Self::PasteRegister(register, extend) => {
                let command_sender = state.lock_state::<CommandSender>().await;
                let text = registers.get(&register.unwrap_or('a')).to_string();
                match command_sender.send(Box::new(BufferCommand::Append(
                    text,
                    extend.unwrap_or(false),
                ))) {
                    Ok(_) => {}
                    Err(e) => {
                        let logger = state.lock_state::<LogSender>().await;

                        logger.critical(
                            "core::register_commands",
                            format!("Failed to send paste command due to error: {e}"),
                        );

                        return false;
                    }
                }

                true
            }
        }
    }
}
