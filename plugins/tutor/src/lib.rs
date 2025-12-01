use kerbin_core::*;

pub mod load;

pub async fn init(state: &mut State) {
    state
        .on_hook(hooks::PostInit)
        .system(load::open_default_buffer);

    state.on_hook(hooks::Update).system(load::update_buffer);
}
