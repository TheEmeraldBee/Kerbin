use crate::*;
use kerbin_core::*;

/// Command to process LSP events for all active clients
pub struct ProcessLspEventsCommand;

#[async_trait::async_trait]
impl kerbin_core::Command for ProcessLspEventsCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let mut lsp_manager = state.lock_state::<LspManager>().await;
        let handler_manager = state.lock_state::<LspHandlerManager>().await;

        // Process events for all active clients
        for (_lang, client) in lsp_manager.client_map.iter_mut() {
            client.process_events(&handler_manager, state).await;
        }

        true
    }
}

/// System that processes LSP events each frame for any opened files
pub async fn process_lsp_events(bufs: Res<Buffers>, command_sender: Res<CommandSender>) {
    get!(bufs, command_sender);

    for buf in &bufs.buffers {
        if buf.read().await.flags.contains("lsp_opened") {
            let _ = command_sender.send(Box::new(ProcessLspEventsCommand));
            break;
        }
    }
}
