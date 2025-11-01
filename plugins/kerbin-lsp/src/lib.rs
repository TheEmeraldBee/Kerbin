use kerbin_core::*;

pub async fn init(state: &mut State) {
    state
        .lock_state::<LogSender>()
        .await
        .low("kerbin-lsp", "Loaded Lsp");
}
