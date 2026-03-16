// Sample config file for basic plugin systems

use kerbin_core::*;

/// Example for subscribing to an event
pub async fn my_test_system(log: Res<LogSender>, event_data: EventData<SaveEvent>) {
    get!(log, Some(event_data));

    log.medium(
        "my-plugin",
        format!("file-saved to path {}!", event_data.path,),
    );
}

pub async fn init(state: &mut State) {
    kerbin_tree_sitter::init(state).await;
    kerbin_lsp::init(state).await;

    /*
    Welcome to the rust-side of your configuration!
    By removing the line below this, the next time you rebuild your configuration,
    The tutor will be **GONE** never to be seen again (Unless you add this line back again)

    After doing this, type `:w`, then subsequently `:q!` in order to write this file,
    then force quit out of the editor, ignoring changes that occurred in the tutor file.

    Finally, run in your shell, `kerbin-install -r -y`. This will reinstall your config into the right place, but without the tutor!

    This will then allow you to use you're editor with everything you've learned

    Good luck on your journey, and of course, if you have any questions,
    feel free to reach out on Github via issues or discussions

    Enjoy you're space-age text editing experience
    */
    tutor::init(state).await;

    EVENT_BUS
        .subscribe::<SaveEvent>()
        .await
        .system(my_test_system);
}
