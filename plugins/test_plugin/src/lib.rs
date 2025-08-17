#![allow(improper_ctypes_definitions)]

use std::sync::Arc;

use kerbin_core::*;
use kerbin_macros::*;

use ascii_forge::prelude::*;

#[kerbin]
pub async fn init(state: Arc<State>) {
    state
        .buffers
        .write()
        .unwrap()
        .open("kerbin/src/main.rs".to_string());
}

#[kerbin]
pub async fn update(state: Arc<State>) {
    render!(state.window.write().unwrap(), (0, 10) => ["Hello".red()]);
}
