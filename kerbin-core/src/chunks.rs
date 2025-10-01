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

#[derive(State)]
pub struct CommandlineChunk;

#[derive(State)]
pub struct CommandSuggestionsChunk;

#[derive(State)]
pub struct CommandDescChunk;

#[derive(State)]
pub struct LogChunk;
