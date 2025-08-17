use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use ascii_forge::prelude::*;

use kerbin_core::*;
use kerbin_plugin::Plugin;

#[tokio::main]
async fn main() {
    handle_panics();
    let window = Window::init().unwrap();

    let my_plugin = Plugin::load("./target/release/libtest_plugin.so");

    let state = Arc::new(State::new(window));
    let _: () = my_plugin.call_async_func(b"init\0", state.clone()).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                // Basic Frame update
                state.window.write().unwrap().update(Duration::ZERO).unwrap();

                state.buffers.write().unwrap().render(vec2(0, 0), state.window.write().unwrap().buffer_mut());

                if event!(state.window.write().unwrap(), Event::Key(k) => k.code == KeyCode::Char('q')) {
                    break;
                }

                if !state.running.load(Ordering::Relaxed) {
                    break
                }
            }
        };
    }

    // Clean up the state
    state
        .window
        .write()
        .unwrap()
        .restore()
        .expect("Window should restore fine");
}
