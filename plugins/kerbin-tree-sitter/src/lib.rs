use kerbin_core::{kerbin_macros::State, *};
use tree_sitter::InputEdit;

use std::collections::BTreeMap;

pub mod grammar;
pub use grammar::*;

pub mod state;
pub use state::*;

#[derive(State, Default)]
pub struct TreeSitterStates {
    pub bufs: BTreeMap<String, Option<TSState>>,
}

pub fn register_lang(
    state: &mut State,
    name: impl ToString,
    exts: impl IntoIterator<Item = impl ToString>,
) {
    let name = name.to_string();

    for ext in exts.into_iter() {
        state
            .on_hook(RenderFiletype::new(ext.to_string()))
            .system(render_tree_sitter_buffer);

        state
            .lock_state::<GrammarManager>()
            .unwrap()
            .register_extension(ext, &name);
    }
}

pub async fn update_ts_buffers(
    states: ResMut<TreeSitterStates>,
    grammars: ResMut<GrammarManager>,
    buffers: Res<Buffers>,

    theme: Res<Theme>,
) {
    get!(mut states, buffers, mut grammars, theme);

    let buf = buffers.cur_buffer();
    let buf = buf.read().unwrap();

    if !states.bufs.contains_key(&buf.path) {
        let grammar = TSState::init(&buf.ext, &buf.rope, &mut grammars, &theme);
        states.bufs.insert(buf.path.clone(), grammar);
    }

    let state = states.bufs.get_mut(&buf.path).unwrap();

    if let Some(ts) = state {
        for change in buf.byte_changes.iter() {
            ts.tree_sitter_dirty = true;
            ts.changes.push(InputEdit {
                start_position: tree_sitter::Point::new(change[0].0.0, change[0].0.1),
                start_byte: change[0].1,

                old_end_position: tree_sitter::Point::new(change[1].0.0, change[1].0.1),
                old_end_byte: change[1].1,

                new_end_position: tree_sitter::Point::new(change[2].0.0, change[2].0.1),
                new_end_byte: change[2].1,
            })
        }

        ts.update_tree_and_highlights(&buf.rope, &theme);
    }
}

pub fn init(state: &mut State) {
    // Register states
    state
        .state(TreeSitterStates::default())
        .state(GrammarManager::default());

    state.on_hook(PostUpdate).system(update_ts_buffers);
}
