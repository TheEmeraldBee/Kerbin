#![allow(improper_ctypes_definitions)]

use std::sync::Arc;

use kerbin_core::*;
use kerbin_macros::*;

#[kerbin]
pub async fn init(state: Arc<State>) {
    state
        .grammar
        .write()
        .unwrap()
        .register_extension("rs", "rust");

    state
        .grammar
        .write()
        .unwrap()
        .register_extension("toml", "toml");
}

#[kerbin]
pub async fn update(_state: Arc<State>) {}
