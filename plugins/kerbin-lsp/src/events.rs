use crate::*;

pub struct ProcessLspEventsCommand;

impl kerbin_core::CommandAny for ProcessLspEventsCommand {
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

#[async_trait::async_trait]
impl kerbin_core::Command<State> for ProcessLspEventsCommand {
    async fn apply(&self, state: &mut State) -> bool {
        // Phase 1: drain messages from all clients while holding LspManager.
        // resolve_request methods are looked up here (needs request_info map).
        let messages = {
            let mut lsp_manager = state.lock_state::<LspManager>().await;
            let mut all = Vec::new();
            for (_lang, client) in lsp_manager.client_map.iter_mut() {
                all.extend(client.drain_messages());
            }
            all
        }; // LspManager lock released here — handlers may now acquire it freely

        if messages.is_empty() {
            return true;
        }

        // Phase 2: dispatch handlers without holding LspManager.
        let handler_manager = state.lock_state::<LspHandlerManager>().await;
        for drained in &messages {
            let handlers = match &drained.message {
                JsonRpcMessage::Response(_) => {
                    handler_manager.iter_response_handlers(&drained.lang_id)
                }
                JsonRpcMessage::Notification(_) => {
                    handler_manager.iter_notification_handlers(&drained.lang_id)
                }
                JsonRpcMessage::ServerRequest(_) => {
                    handler_manager.iter_server_request_handlers(&drained.lang_id)
                }
            };
            LspClient::<tokio::process::ChildStdin>::call_matching_handlers(handlers, &drained.method, state, &drained.message)
                .await;
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
