use std::{fs::File, sync::Mutex, time::Duration, usize};

use ascii_forge::prelude::*;

use crokey::{
    Combiner,
    crossterm::{
        cursor::{MoveTo, SetCursorStyle, Show},
        *,
    },
    key,
};
use stategine::{prelude::*, system::into_system::IntoSystem};
use tracing::Level;

use zellix::*;

fn update_window(mut window: ResMut<Window>) {
    window.update(Duration::from_millis(10)).unwrap();
}

fn render_cursor(mut window: ResMut<Window>, mut buffers: ResMut<Buffers>, mode: Res<Mode>) {
    let mut cursor_pos = buffers.cur_buffer_mut().cursor_pos;
    cursor_pos.y += 1;

    let cursor_style = match mode.0 {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

    execute!(
        window.io(),
        MoveTo(cursor_pos.x, cursor_pos.y),
        cursor_style,
        Show,
    )
    .unwrap();
}

fn main() {
    let log_file = File::options()
        .create(true)
        .append(true)
        .write(true)
        .open("zellix.log")
        .expect("file should be able to open");

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(Level::INFO)
        .with_writer(Mutex::new(log_file))
        .init();

    let window = Window::init().unwrap();
    handle_panics();

    let combiner = Combiner::default();

    let buffers = Buffers {
        selected_buffer: 0,
        buffers: vec![TextBuffer::scratch()],
    };

    let mut input_config = InputConfig::default();

    // ----------------------- //
    // Temporary Keybind Setup //
    // ----------------------- //

    // Command Palette
    input_config.register_input(
        ['n'],
        [key!(':')],
        [EditorCommand::ChangeMode('c')],
        "Command Palette",
    );

    let mut plugin_manager = PluginManager::new().expect("Failed to create plugin manager");
    plugin_manager
        .load_plugins()
        .expect("Failed to load plugins");

    let mut engine = Engine::new();
    engine.states((Running(true), Mode::default()));
    engine.states((window, combiner, buffers));
    engine.states((
        input_config,
        InputState::default(),
        CommandPaletteState::new(),
        plugin_manager,
    ));

    engine.systems((
        handle_inputs,
        render_buffers,
        render_help_menu,
        handle_command_palette_input,
        render_command_palette,
        run_plugin_render_hooks,
    ));

    // Run load scripts on all plugins
    engine.oneshot_system(run_plugin_load_hooks.into_system());

    while engine.get_state_mut::<Running>().0 == true {
        engine.update();

        // These are updated seperately because they want commands to be applied
        engine.oneshot_system(update_window.into_system());
        engine.oneshot_system(render_cursor.into_system());
    }

    engine.get_state_mut::<Window>().restore().unwrap();
}
