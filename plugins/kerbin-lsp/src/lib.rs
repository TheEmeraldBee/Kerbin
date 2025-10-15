use kerbin_core::State;

pub mod jsonrpc;
pub use jsonrpc::*;

pub mod client;
pub use client::*;

pub mod uriext;
pub use uriext::*;

pub mod facade;
pub use facade::*;

pub mod manager;
pub use manager::*;

pub mod handlers;
use handlers::*;

// Re-Exports
pub use lsp_types::*;

pub fn init(state: &mut State) {
    state
        .state(LspHandlerManager::default())
        .state(LspManager::default());
}
