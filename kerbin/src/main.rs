use std::{iter::once, panic, str::FromStr, time::Duration};

use ascii_forge::prelude::*;

use ipmpsc::SharedRingBuffer;
use kerbin_config::Config;
use kerbin_core::*;

use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};
use tokio::sync::mpsc::unbounded_channel;

use clap::*;
use uuid::Uuid;

#[derive(Parser, Clone)]
pub struct KerbinCommand {
    /// A list of commands you would like to run
    #[clap(short, long)]
    cmd: Vec<String>,

    /// The session to pass these commands to,
    /// Not putting one will create a new session and run the commands.
    #[clap(short, long)]
    session: Option<String>,
}

#[derive(Subcommand, Clone)]
pub enum KerbinSubcommand {
    /// Run a command either in a new session, or in the defined session
    Command(KerbinCommand),

    /// Run a command, then quit the editor
    /// This can be done in a new window, or in a session by using the `-s` flag
    ///
    /// Should mostly be used to run one off commands like install commands.
    CommandQuit(KerbinCommand),
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
/// Kerbin: The Space-Age Text Editor
pub struct KerbinArgs {
    /// Defines a path to the config, using default if not provided
    #[clap(short, long)]
    config: Option<String>,
    #[clap(subcommand)]
    subcommand: Option<KerbinSubcommand>,
}

pub async fn register_default_chunks(chunks: ResMut<Chunks>, window: Res<WindowState>) {
    get!(mut chunks, window);

    let layout = Layout::new()
        .row(fixed(1), vec![flexible()])
        .row(flexible(), vec![fixed(5), fixed(2), flexible()])
        .row(fixed(1), vec![flexible()])
        .row(fixed(1), vec![flexible()])
        .calculate(window.size())
        .unwrap();

    chunks.register_chunk::<BufferlineChunk>(0, layout[0][0]);
    chunks.register_chunk::<BufferGutterChunk>(0, layout[1][0]);
    chunks.register_chunk::<BufferChunk>(0, layout[1][2]);
    chunks.register_chunk::<StatuslineChunk>(0, layout[2][0]);
}

pub async fn render_chunks(chunks: Res<Chunks>, window: ResMut<WindowState>) {
    get!(chunks, mut window);

    let mut best_cursor = None;

    for layer in &chunks.buffers {
        for buffer in layer {
            let buf = buffer.1.read().await;
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

            buf.render(buffer.0, window.buffer_mut());

            for (offset, item) in &buf.render_items {
                let absolute_pos = buffer.0 + *offset;

                item(&mut window, absolute_pos);
            }
        }
    }

    if let Some((_, pos, sty)) = best_cursor {
        window.set_cursor_visible(true);
        window.set_cursor(pos);
        window.set_cursor_style(sty);
    } else {
        window.set_cursor_visible(false);
    }
}

async fn update(state: &mut State) {
    // Update all states
    state.hook(hooks::Update).call().await;

    state.hook(hooks::PostUpdate).call().await;

    // Clear the chunks for the next frame (allows for conditional chunks)
    state.lock_state::<Chunks>().await.clear();

    // Register all chunks for rendering
    state.hook(hooks::ChunkRegister).call().await;

    // Call the file renderer
    let filetype = {
        let bufs = state.lock_state::<Buffers>().await;
        bufs.cur_buffer().await.ext.clone()
    };

    state
        .hook(hooks::UpdateFiletype::new(filetype))
        .call()
        .await;

    state.hook(hooks::UpdateCleanup).call().await;

    state.hook(hooks::PreLines).call().await;

    state
        .hook(hooks::CreateRenderLines)
        .hook(hooks::PreRender)
        .call()
        .await;

    state.hook(hooks::Render).call().await;

    // Render all chunks to the window
    state.hook(hooks::RenderChunks).call().await;

    // Call out to recently emited events
    EVENT_BUS.resolve(state).await;
}

#[tokio::main]
async fn main() {
    init_log();

    let mut new_session = true;
    let mut command_session = Uuid::new_v4();
    let mut command_strings = vec![];

    let args = KerbinArgs::parse();

    match args.subcommand {
        Some(KerbinSubcommand::Command(t)) => {
            command_strings.extend(t.cmd);
            if let Some(s) = t.session {
                command_session = Uuid::from_str(&s).unwrap();
                new_session = false;
            }
        }
        Some(KerbinSubcommand::CommandQuit(t)) => {
            command_strings.extend(t.cmd.into_iter().chain(once("q!".to_string())));
            if let Some(s) = t.session {
                command_session = Uuid::from_str(&s).unwrap();
                new_session = false;
            }
        }
        _ => {}
    }

    let config_path = match args.config {
        Some(t) => t,
        None => {
            // Check XDG_CONFIG_HOME environment variable first (favors ~/.config)
            let mut res = if let Ok(home) = std::env::var("XDG_CONFIG_HOME") {
                std::path::PathBuf::from(home)
            } else {
                // Fallback to the OS-native directory (via the `dirs` crate)
                dirs::config_dir().expect("Failed to find user config directory")
            };

            res.push("kerbin"); // Append application name
            res.to_string_lossy().to_string()
        }
    };

    let temp_dir = dirs::data_dir().unwrap();
    let queue_dir = format!("{}/kerbin/sessions", temp_dir.to_string_lossy());
    let queue_file = format!("{}/{}", queue_dir, command_session);

    if !new_session {
        let queue = ipmpsc::Sender::new(SharedRingBuffer::open(&queue_file).unwrap());
        for cmd in command_strings {
            queue.send(&cmd).unwrap();
        }

        return;
    }

    std::fs::create_dir_all(queue_dir).unwrap();

    let queue = ipmpsc::Receiver::new(SharedRingBuffer::create(&queue_file, 16000).unwrap());

    let window = Window::init().unwrap();
    handle_panics();

    let (command_sender, mut command_reciever) = unbounded_channel();

    let mut state = init_state(window, command_sender, config_path.clone(), command_session);
    resolver_engine_mut()
        .await
        .set_template("session", [command_session]);

    let mut framerate = 60;

    match Config::load(format!("{config_path}/config/config.toml")) {
        Ok(t) => {
            framerate = t.core.framerate();
            t.apply(&mut state).await;
        }
        Err(e) => {
            state
                .lock_state::<LogSender>()
                .await
                .critical("core::config_load", e);
        }
    }

    let ms_per_frame = 1000 / framerate;

    config::init(&mut state).await;

    // Register command types
    {
        let mut commands = state.lock_state::<CommandRegistry>().await;

        commands.register::<BufferCommand>();
        commands.register::<CommitCommand>();

        commands.register::<CursorCommand>();

        commands.register::<BuffersCommand>();

        commands.register::<ModeCommand>();
        commands.register::<StateCommand>();

        commands.register::<MotionCommand>();

        commands.register::<ShellCommand>();

        commands.register::<RegisterCommand>();
    }

    {
        let commands = state.lock_state::<CommandRegistry>().await;
        let command_sender = state.lock_state::<CommandSender>().await;
        let prefix_registry = state.lock_state::<CommandPrefixRegistry>().await;
        let modes = state.lock_state::<ModeStack>().await;

        for string in command_strings {
            let words = word_split(&string);
            if let Some(cmd) = commands.parse_command(
                words,
                true,
                false,
                Some(&resolver_engine().await.as_resolver()),
                true,
                &prefix_registry,
                &modes,
            ) {
                command_sender.send(cmd).unwrap();
            }
        }
    }

    state
        .on_hook(hooks::ChunkRegister)
        .system(register_default_chunks)
        .system(register_command_palette_chunks)
        .system(register_log_chunk)
        .system(register_help_menu_chunk);

    state
        .on_hook(hooks::Update)
        .system(update_debounce)
        .system(handle_inputs)
        .system(handle_command_palette_input)
        .system(update_palette_suggestions)
        .system(render_cursors_and_selections);

    state.on_hook(hooks::PostUpdate).system(post_update_buffer);

    state
        .on_hook(hooks::CreateRenderLines)
        .system(build_buffer_lines);

    state
        .on_hook(hooks::PreLines)
        .system(update_buffer_horizontal_scroll)
        .system(update_buffer_vertical_scroll);

    state
        .on_hook(hooks::Render)
        .system(render_statusline)
        .system(render_command_palette)
        .system(render_help_menu)
        .system(render_bufferline)
        .system(render_log)
        .system(render_buffer_default);

    state.on_hook(hooks::UpdateCleanup).system(cleanup_buffers);

    state.on_hook(hooks::RenderChunks).system(render_chunks);

    loop {
        let frame_start = tokio::time::Instant::now();

        {
            let commands = state.lock_state::<CommandRegistry>().await;
            let command_sender = state.lock_state::<CommandSender>().await;
            let prefix_registry = state.lock_state::<CommandPrefixRegistry>().await;
            let modes = state.lock_state::<ModeStack>().await;

            while let Some(item) = queue.try_recv::<String>().unwrap() {
                let words = word_split(&item);
                if let Some(cmd) = commands.parse_command(
                    words,
                    true,
                    false,
                    Some(&resolver_engine().await.as_resolver()),
                    true,
                    &prefix_registry,
                    &modes,
                ) {
                    command_sender.send(cmd).unwrap();
                }
            }
        }

        // Process all available commands with blocking
        while let Ok(cmd) = command_reciever.try_recv() {
            cmd.apply(&mut state).await;
        }

        // Run the update cycle
        update(&mut state).await;

        if !state.lock_state::<Running>().await.0 {
            break;
        }

        // Sleep for remaining time while handling commands
        let target_frame_time = Duration::from_millis(ms_per_frame);
        let deadline = frame_start + target_frame_time;

        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            tokio::select! {
                Some(cmd) = command_reciever.recv() => {
                    cmd.apply(&mut state).await;
                }
                _ = tokio::time::sleep(remaining) => {
                    break;
                }
            }
        }

        {
            let mut window = state.lock_state::<WindowState>().await;
            match window.update(Duration::from_millis(0)) {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("Window failed to update: {e:?}");
                }
            }
        }
    }

    state
        .lock_state::<WindowState>()
        .await
        .restore()
        .expect("Window should restore fine");

    let _ = std::fs::remove_file(&queue_file);
}
