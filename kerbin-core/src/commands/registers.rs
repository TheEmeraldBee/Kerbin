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
    /// Use --extend to extend the selection to the pasted text
    PasteRegister(
        #[command(type_name = "char?", name = "register")] Option<char>,
        #[command(flag, name = "extend")] bool,
    ),

    #[command(name = "clipboard-copy")]
    /// Copy the selection to the OS clipboard
    ClipboardCopy,

    #[command(name = "clipboard-paste")]
    /// Paste from the OS clipboard
    ///
    /// Use --extend to extend the selection to the pasted text
    ClipboardPaste(#[command(flag, name = "extend")] bool),
}

#[async_trait::async_trait]
impl Command for RegisterCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut registers = state.lock_state::<Registers>().await;

        match self {
            Self::CopyRegister(register) => {
                let buf = state.lock_state::<Buffers>().await.cur_buffer().await;

                let byte_range = buf.primary_cursor().sel().clone();
                let text = buf
                    .slice_to_string(*byte_range.start(), *byte_range.end())
                    .unwrap_or_default();

                registers.set(register.unwrap_or('a'), text);

                true
            }
            Self::PasteRegister(register, extend) => {
                let command_sender = state.lock_state::<CommandSender>().await;
                let text = registers.get(&register.unwrap_or('a')).to_string();
                match command_sender.send(Box::new(BufferCommand::Append {
                    text,
                    extend: *extend,
                })) {
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
            Self::ClipboardCopy => {
                let buf = state.lock_state::<Buffers>().await.cur_buffer().await;
                let byte_range = buf.primary_cursor().sel().clone();
                let text = buf
                    .slice_to_string(*byte_range.start(), *byte_range.end())
                    .unwrap_or_default();
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                    Ok(_) => true,
                    Err(e) => {
                        let logger = state.lock_state::<LogSender>().await;
                        logger.critical(
                            "core::register_commands",
                            format!("Failed to copy to OS clipboard: {e}"),
                        );
                        false
                    }
                }
            }
            Self::ClipboardPaste(extend) => {
                let text = match arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                    Ok(t) => t,
                    Err(e) => {
                        let logger = state.lock_state::<LogSender>().await;
                        logger.critical(
                            "core::register_commands",
                            format!("Failed to read from OS clipboard: {e}"),
                        );
                        return false;
                    }
                };
                let command_sender = state.lock_state::<CommandSender>().await;
                match command_sender.send(Box::new(BufferCommand::Append {
                    text,
                    extend: *extend,
                })) {
                    Ok(_) => {}
                    Err(e) => {
                        let logger = state.lock_state::<LogSender>().await;
                        logger.critical(
                            "core::register_commands",
                            format!("Failed to send clipboard paste command due to error: {e}"),
                        );
                        return false;
                    }
                }
                true
            }
        }
    }
}
