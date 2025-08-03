use std::{cell::RefCell, fs::File, path::PathBuf, rc::Rc, sync::Mutex, time::Duration};

use ascii_forge::prelude::*;

use crokey::{
    Combiner,
    crossterm::{
        cursor::{Hide, MoveTo, SetCursorStyle, Show},
        *,
    },
};
use ipmpsc::SharedRingBuffer;
use stategine::{prelude::*, system::into_system::IntoSystem};
use tracing::Level;

use kerbin::{buffer_extensions::BufferExtension, *};

fn update_window(mut window: ResMut<Window>) {
    window.update(Duration::from_millis(10)).unwrap();
}

fn render_cursor(mut window: ResMut<Window>, buffers: Res<Buffers>, mode: Res<Mode>) {
    let mut cursor_pos = buffers.cur_buffer().borrow().cursor_pos;
    let scroll = buffers.cur_buffer().borrow().scroll;

    if scroll as u16 > cursor_pos.y {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    cursor_pos.y += 1;

    cursor_pos.y = cursor_pos
        .y
        .saturating_sub(buffers.cur_buffer().borrow().scroll as u16);

    cursor_pos.x = cursor_pos
        .x
        .saturating_sub(buffers.cur_buffer().borrow().h_scroll as u16);

    if cursor_pos.y > window.size().y {
        execute!(window.io(), Hide).unwrap();
        return;
    }

    let cursor_style = match mode.0 {
        'i' => SetCursorStyle::SteadyBar,
        _ => SetCursorStyle::SteadyBlock,
    };

    window.buffer_mut().style_line(cursor_pos.y, |s| {
        s.on(Color::Rgb {
            r: 40,
            g: 40,
            b: 56,
        })
    });

    execute!(
        window.io(),
        MoveTo(cursor_pos.x + 6, cursor_pos.y),
        cursor_style,
        Show,
    )
    .unwrap();
}

fn main() {
    // Generate the session id that will be passed to all sh calls.
    let session_id = uuid::Uuid::new_v4().to_string();

    let path = format!(
        "{}/kerbin/sessions/{}",
        dirs::data_dir().unwrap().display(),
        session_id
    );

    let mut dir = PathBuf::from(&path);
    dir.pop();

    let _ = std::fs::create_dir_all(&dir);

    let receiver = ipmpsc::Receiver::new(SharedRingBuffer::create(&path, 32 * 1024).unwrap());

    let link = ShellLink {
        session_id,
        receiver,
    };

    let log_file = File::options()
        .create(true)
        .append(true)
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
        tab_scroll: 0,
        buffers: vec![Rc::new(RefCell::new(TextBuffer::scratch()))],
    };

    let input_config = InputConfig::default();

    // ----------------------- //
    // Temporary Keybind Setup //
    // ----------------------- //

    let grammar_manager = GrammarManager::new();
    let theme = Theme::default();

    let mut plugin_manager = ConfigManager::new().expect("Failed to create plugin manager");
    let res = plugin_manager.load_config();

    match res {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("{e}");
        }
    }

    let mut engine = Engine::new();
    engine.states((Running(true), Mode::default()));
    engine.states((window, combiner, buffers));
    engine.states((
        input_config,
        InputState::default(),
        CommandPaletteState::new(),
        CommandStatus::default(),
        PluginConfig::default(),
    ));

    engine.states((grammar_manager, theme));

    engine.state(link);

    engine.systems((handle_inputs, handle_command_palette_input));

    engine.systems((
        update_highlights,
        render_buffers,
        render_help_menu,
        render_command_palette,
    ));

    engine.systems((catch_events,));

    if let Err(e) = plugin_manager.run_load_hook(&mut engine) {
        tracing::error!("Rune VM Error: {}", e);
    }

    if let Err(e) = plugin_manager.run_load_languages_hook(&mut engine) {
        tracing::error!("Rune VM Error: {}", e);
    }

    while engine.get_state_mut::<Running>().0 {
        engine.update();

        if let Err(e) = plugin_manager.run_update_hook(&mut engine) {
            tracing::error!("Rune VM Error: {}", e);
        }

        // These are updated seperately because they want commands to be applied
        engine.oneshot_system(update_buffer.into_system());
        engine.oneshot_system(update_bufferline_scroll.into_system());
        engine.oneshot_system(update_window.into_system());
        engine.oneshot_system(render_cursor.into_system());
    }

    let _ = std::fs::remove_file(format!(
        "{}/kerbin/sessions/{}",
        dirs::data_dir().unwrap().display(),
        engine.take_state::<ShellLink>().session_id
    ));

    engine.get_state_mut::<Window>().restore().unwrap();
}
