#![allow(improper_ctypes_definitions)]

use kerbin_core::{GrammarManager, ResMut, State, SystemParam};

#[unsafe(no_mangle)]
pub async fn hi(grammars: ResMut<GrammarManager>) {
    let mut grammars = grammars.get();
    grammars.register_extension("rs", "rust");
    grammars.register_extension("toml", "toml");
}

#[unsafe(no_mangle)]
pub fn init(state: &mut State) {
    state.on_hook::<bool>().system(hi);
}
