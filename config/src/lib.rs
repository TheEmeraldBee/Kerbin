// Sample config file for basic plugin systems

use kerbin_core::*;
use kerbin_lsp::LangInfo;

/// Example for subscribing to an event
pub async fn my_test_system(log: Res<LogSender>, event_data: EventData<SaveEvent>) {
    get!(log, Some(event_data));

    log.medium(
        "my-plugin",
        format!("file-saved to path {}!", event_data.path,),
    );
}

pub async fn init(state: &mut State) {
    // Initialize the tree-sitter plugin
    kerbin_tree_sitter::init(state).await;

    // Initialize the lsp plugin
    kerbin_lsp::init(state).await;

    kerbin_lsp::register_lang(
        state,
        "rust",
        ["rs"],
        LangInfo::new("rust-analyzer")
            .with_root("Cargo.toml") // Root paths
            .with_root("Cargo.lock"),
    )
    .await;

    tutor::init(state).await;

    EVENT_BUS
        .subscribe::<SaveEvent>()
        .await
        .system(my_test_system);
}
