use kerbin_macros::State;
use kerbin_state_machine::storage::*;

#[derive(State)]
pub struct BufferlineChunk;

#[derive(State)]
pub struct BufferChunk;

#[derive(State)]
pub struct HelpChunk;

#[derive(State)]
pub struct StatuslineChunk;
