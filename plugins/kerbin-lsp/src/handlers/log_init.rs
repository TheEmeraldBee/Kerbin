use crate::*;

pub async fn log_init(state: &State, msg: &JsonRpcMessage) {
    let log = state.lock_state::<LogSender>().await;
    if let crate::JsonRpcMessage::Notification(notif) = msg
        && let Some(value) = notif.params.get("value")
        && let Some(kind) = value.get("kind").and_then(|k| k.as_str())
    {
        match kind {
            "begin" => {
                if let Some(title) = value.get("title").and_then(|t| t.as_str()) {
                    log.medium("lsp::client", format!("[Progress] {}", title));
                }
            }
            "end" => {}
            _ => {}
        }
    }
}
