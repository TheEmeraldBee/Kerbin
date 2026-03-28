use crate::*;
use kerbin_core::*;

pub struct ProcessLspEventsCommand;

impl kerbin_core::CommandAny for ProcessLspEventsCommand {
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

#[async_trait::async_trait]
impl kerbin_core::Command for ProcessLspEventsCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut lsp_manager = state.lock_state::<LspManager>().await;
        let handler_manager = state.lock_state::<LspHandlerManager>().await;

        for (_lang, client) in lsp_manager.client_map.iter_mut() {
            client.process_events(&handler_manager, state).await;
        }

        true
    }
}

pub async fn process_lsp_events(bufs: Res<Buffers>, command_sender: Res<CommandSender>) {
    get!(bufs, command_sender);

    for buf in &bufs.buffers {
        let buf_guard = buf.read().await;
        if let Some(text_buf) = buf_guard.downcast::<TextBuffer>()
            && text_buf.flags.contains("lsp_opened")
        {
            let _ = command_sender.send(Box::new(ProcessLspEventsCommand));
            break;
        }
    }
}
