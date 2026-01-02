use kerbin_state_machine::State;

use crate::*;

pub use serde_json::json;

pub async fn register_default_queries(state: &mut State) {
    let mut registry = state.lock_state::<QueryRegistry>().await;

    registry.register("file_info", move |state| Box::pin(file_info_query(state)));
}

pub async fn file_info_query(state: &mut State) -> String {
    let bufs = state.lock_state::<Buffers>().await;
    let buf = bufs.cur_buffer().await;

    json!({
        "path": buf.path,
        "cursors": buf.cursors.len(),
        "primary_cursor_pos": buf.primary_cursor().get_cursor_byte(),
    })
    .to_string()
}
