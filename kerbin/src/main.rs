use std::{path::PathBuf, time::Duration};

use ratatui::{
    crossterm::execute,
    layout::{Constraint, Layout, Position},
};

use kerbin_core::*;

use kerbin_state_machine::system::param::{SystemParam, res::Res, res_mut::ResMut};
use tokio::sync::mpsc::unbounded_channel;

use clap::*;
use uuid::Uuid;

#[derive(Parser)]
#[command(version, about, long_about = None)]
/// Kerbin: The Space-Age Text Editor
pub struct KerbinArgs {
    /// Defines a path to the config, using default if not provided
    #[clap(short, long)]
    config: Option<String>,

    /// Files to open on startup
    #[clap(value_name = "FILE")]
    files: Vec<std::path::PathBuf>,
}

pub async fn register_default_chunks(chunks: ResMut<Chunks>, window: Res<WindowState>) {
    get!(mut chunks, window);

    let size = window.size();

    let [bufferline, main_area, statusline] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(size);

    let [gutter, _pad, buffer] = Layout::horizontal([
        Constraint::Length(5),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(main_area);

    chunks.register_chunk::<BufferlineChunk>(0, bufferline);
    chunks.register_chunk::<BufferGutterChunk>(0, gutter);
    chunks.register_chunk::<BufferChunk>(0, buffer);
    chunks.register_chunk::<StatuslineChunk>(0, statusline);
}

pub async fn render_chunks(chunks: Res<Chunks>, window: ResMut<WindowState>) {
    get!(chunks, mut window);

    let mut best_cursor: Option<(usize, u16, u16, CursorShape)> = None;

    // Collect best cursor from all chunk layers
    for layer in &chunks.buffers {
        for chunk_arc in layer {
            let chunk = chunk_arc.read().await;
            if let Some(cur) = chunk.get_cursor() {
                let replace = best_cursor
                    .map(|(priority, _, _, _)| cur.0 > priority)
                    .unwrap_or(true);
                if replace {
                    best_cursor = Some(*cur);
                }
            }
        }
    }

    tokio::task::block_in_place(|| {
        window.0.draw(|frame| {
            // Blit all chunk buffers into frame
            for layer in &chunks.buffers {
                for chunk_arc in layer {
                    let chunk = chunk_arc.blocking_read();
                    blit(&chunk, frame.buffer_mut());
                }
            }

            if let Some((_, x, y, _)) = best_cursor {
                frame.set_cursor_position(Position::new(x, y));
            }
        })
    })
    .ok();

    // Apply cursor shape after draw
    if let Some((_, _, _, shape)) = best_cursor {
        execute!(std::io::stdout(), shape.to_crossterm_style()).ok();
    }
}

fn blit(src: &ratatui::buffer::Buffer, dst: &mut ratatui::buffer::Buffer) {
    let src_area = src.area;
    for y in src_area.top()..src_area.bottom() {
        for x in src_area.left()..src_area.right() {
            if let Some(src_cell) = src.cell((x, y))
                && let Some(dst_cell) = dst.cell_mut((x, y))
            {
                *dst_cell = src_cell.clone();
            }
        }
    }
}

async fn update(state: &mut State) {
    // Poll crossterm events and store in CrosstermEvents state
    {
        let mut events_state = state.lock_state::<CrosstermEvents>().await;
        events_state.0.clear();
        while ratatui::crossterm::event::poll(Duration::ZERO).unwrap_or(false) {
            if let Ok(event) = ratatui::crossterm::event::read() {
                if let ratatui::crossterm::event::Event::Resize(_, _) = &event {
                    // autoresize handled by terminal on next draw
                }
                events_state.0.push(event);
            }
        }
    }

    state.hook(hooks::Update).call().await;
    state.hook(hooks::PostUpdate).call().await;

    // Clear chunks for the next frame
    state.lock_state::<Chunks>().await.clear();

    state.hook(hooks::ChunkRegister).call().await;

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

    state.hook(hooks::PreRender).call().await;

    state.hook(hooks::Render).call().await;

    state.hook(hooks::RenderChunks).call().await;

    handle_ipc_messages(state).await;

    EVENT_BUS.resolve(state).await;
}

#[tokio::main]
async fn main() {
    let args = KerbinArgs::parse();

    let session_id = Uuid::new_v4();
    let server_ipc = ServerIpc::new(&session_id.to_string());

    let config_path = match args.config {
        Some(t) => {
            if let Ok(p) = std::fs::canonicalize(&t) {
                p.to_string_lossy().to_string()
            } else {
                t
            }
        }
        None => {
            if let Ok(home) = std::env::var("XDG_CONFIG_HOME") {
                let mut path = std::path::PathBuf::from(home);
                path.push("kerbin");
                path.to_string_lossy().to_string()
            } else {
                let home_config = dirs::home_dir().map(|h| h.join(".config").join("kerbin"));

                if let Some(path) = home_config.filter(|p| p.exists()) {
                    path.to_string_lossy().to_string()
                } else {
                    let mut res = dirs::config_dir().expect("Failed to find user config directory");
                    res.push("kerbin");
                    res.to_string_lossy().to_string()
                }
            }
        }
    };

    resolver_engine_mut()
        .await
        .set_template("cfg_folder", [&config_path]);

    resolver_engine_mut()
        .await
        .set_template("session", [session_id]);

    init_log();

    let terminal = ratatui::init();

    // Initialize terminal
    let (command_sender, mut command_reciever) = unbounded_channel();

    let mut state = init_state(
        terminal,
        command_sender,
        config_path.clone(),
        session_id,
        server_ipc,
    );

    {
        let mut commands = state.lock_state::<CommandRegistry>().await;

        commands.register::<BufferCommand>();
        commands.register::<CommitCommand>();

        commands.register::<CursorCommand>();

        commands.register::<BuffersCommand>();

        commands.register::<ModeCommand>();
        commands.register::<StateCommand>();

        commands.register::<PaletteCommand>();
        commands.register::<InputCommand>();

        commands.register::<MotionCommand>();

        commands.register::<ShellCommand>();

        commands.register::<RegisterCommand>();

        commands.register::<ConfigCommand>();

        commands.register::<AutoPairsCommand>();
    }

    state
        .lock_state::<CommandInterceptorRegistry>()
        .await
        .on_command::<BufferCommand>(|cmd, state| {
            Box::pin(auto_pairs_intercept(cmd, state))
        });

    config::init(&mut state).await;

    let kb_path = PathBuf::from(format!("{config_path}/config/config.kb"));
    if let Err(e) = load_kb(&kb_path, &mut state).await {
        state
            .lock_state::<LogSender>()
            .await
            .critical("core::config_load", e);
    }

    let framerate = state.lock_state::<CoreConfig>().await.framerate;
    let ms_per_frame = 1000 / framerate;

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
        .system(update_palette_suggestions)
        .system(render_cursors_and_selections);

    state
        .on_hook(hooks::PostUpdate)
        .system(post_update_buffer)
        .system(update_tab_width_template);

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

    state.hook(hooks::PostInit).call().await;

    for file in args.files {
        let path = file.to_string_lossy().to_string();
        state.lock_state::<Buffers>().await.open(path).await.ok();
    }

    loop {
        let frame_start = tokio::time::Instant::now();

        while let Ok(cmd) = command_reciever.try_recv() {
            dispatch_command(cmd, &mut state).await;
        }

        update(&mut state).await;

        if !state.lock_state::<Running>().await.0 {
            break;
        }

        let target_frame_time = Duration::from_millis(ms_per_frame);
        let deadline = frame_start + target_frame_time;

        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            tokio::select! {
                Some(cmd) = command_reciever.recv() => {
                    dispatch_command(cmd, &mut state).await;
                }
                _ = tokio::time::sleep(remaining) => {
                    break;
                }
            }
        }
    }

    // Restore terminal
    ratatui::restore();
}