use std::{panic, time::Duration};

use ascii_forge::{
    prelude::*,
    window::crossterm::{
        cursor::{Hide, MoveTo, Show},
        execute,
    },
};

use kerbin_config::Config;
use kerbin_core::*;

use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};
use tokio::sync::mpsc::unbounded_channel;

pub async fn register_default_chunks(chunks: ResMut<Chunks>, window: Res<WindowState>) {
    get!(mut chunks, window);

    let layout = Layout::new()
        .row(fixed(1), vec![flexible()])
        .row(flexible(), vec![flexible()])
        .row(fixed(1), vec![flexible()])
        .row(fixed(1), vec![flexible()])
        .calculate(window.size())
        .unwrap();

    chunks.register_chunk::<BufferlineChunk>(0, layout[0][0]);
    chunks.register_chunk::<BufferChunk>(0, layout[1][0]);
    chunks.register_chunk::<StatuslineChunk>(0, layout[2][0]);
}

pub async fn render_chunks(chunks: Res<Chunks>, window: ResMut<WindowState>) {
    get!(chunks, mut window);

    let mut best_cursor = None;

    for layer in &chunks.buffers {
        for buffer in layer {
            let buf = buffer.1.read().unwrap();
            if let Some(cur) = buf.get_full_cursor() {
                let replace = match best_cursor {
                    Some((i, _, _)) => cur.0 > i,
                    None => true,
                };

                if replace {
                    // Add the buffer offset to the cursor pos
                    let x = cur.1.x.min(buf.size().x) + buffer.0.x;
                    let y = cur.1.y.min(buf.size().y) + buffer.0.y;
                    best_cursor = Some((cur.0, vec2(x, y), cur.2));
                }
            }
            render!(window, buffer.0 => [ &buf ]);
        }
    }

    if let Some((_, pos, sty)) = best_cursor {
        if window.mouse_pos() != pos {
            execute!(window.io(), Show, MoveTo(pos.x, pos.y), sty).unwrap();
        }
    } else {
        execute!(window.io(), Hide, MoveTo(0, 0)).unwrap();
    }
}

async fn update(state: &mut State) {
    // Update all states
    state.hook(Update).call().await;

    state.hook(PostUpdate).call().await;

    state.hook(UpdateCleanup).call().await;

    // Clear the chunks for the next frame (allows for conditional chunks)
    state.lock_state::<Chunks>().unwrap().clear();

    // Register all chunks for rendering
    state.hook(ChunkRegister).call().await;

    // Call the file renderer
    let filetype = {
        let bufs = state.lock_state::<Buffers>().unwrap();
        bufs.cur_buffer().read().unwrap().ext.clone()
    };

    // Render the rest of the state (This will chain rendering the file,
    // allows for many async systems to run at "once" (async, non-threaded)
    state
        .hook(RenderFiletype::new(filetype))
        .hook(Render)
        .call()
        .await;

    // Render all chunks to the window
    state.hook(RenderChunks).call().await;

    match state
        .lock_state::<WindowState>()
        .unwrap()
        .update(Duration::from_millis(0))
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("{e}");
        }
    }
}

#[tokio::main]
async fn main() {
    init_log();

    handle_panics();
    let window = Window::init().unwrap();

    let (command_sender, mut command_reciever) = unbounded_channel();

    let mut state = init_state(window, command_sender);

    let config = Config::load("./config/config/config.toml").unwrap();

    config.apply(&mut state);

    let plugin = Plugin::load("./target/release/libconfig.so");

    // Run the plugin's init
    let _: () = plugin.call_func(b"init", &mut state);

    // Register command types
    {
        let mut commands = state.lock_state::<CommandRegistry>().unwrap();

        commands.register::<BufferCommand>();
        commands.register::<CommitCommand>();

        commands.register::<CursorCommand>();

        commands.register::<BuffersCommand>();

        commands.register::<ModeCommand>();
        commands.register::<StateCommand>();

        commands.register::<MotionCommand>();

        commands.register::<ShellCommand>();
    }

    state
        .on_hook(ChunkRegister)
        .system(register_default_chunks)
        .system(register_command_palette_chunks)
        .system(register_help_menu_chunk);

    state
        .on_hook(Update)
        .system(handle_inputs)
        .system(handle_command_palette_input)
        .system(update_palette_suggestions);

    state.on_hook(UpdateCleanup).system(update_buffer);

    state
        .on_hook(Render)
        .system(render_statusline)
        .system(render_command_palette)
        .system(render_help_menu)
        .system(render_bufferline);

    state
        .on_hook(RenderFiletype::new("*"))
        .system(render_buffer_default);

    state.on_hook(RenderChunks).system(render_chunks);

    loop {
        let frame_start = tokio::time::Instant::now();

        // Process all available commands with blocking
        while let Ok(cmd) = command_reciever.try_recv() {
            cmd.apply(&mut state);
        }

        // Run the update cycle
        update(&mut state).await;

        if !state.lock_state::<Running>().unwrap().0 {
            break;
        }

        // Sleep for remaining time while handling commands
        let target_frame_time = Duration::from_millis(12);
        let deadline = frame_start + target_frame_time;

        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());

            tokio::select! {
                Some(cmd) = command_reciever.recv() => {
                    cmd.apply(&mut state);
                }
                _ = tokio::time::sleep(remaining) => {
                    break;
                }
            }
        }
    }

    state
        .lock_state::<WindowState>()
        .unwrap()
        .restore()
        .expect("Window should restore fine");

    // State **MUST** Be dropped before the plugin,
    // as state may store references to memory in state
    drop(state);
    drop(plugin);
}
