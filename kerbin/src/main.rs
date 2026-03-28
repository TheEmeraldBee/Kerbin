use std::{path::PathBuf, time::Duration};

use ratatui::{
    crossterm::{
        event::{EnableBracketedPaste, EnableMouseCapture},
        execute,
    },
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
    files: Vec<PathBuf>,
}

fn collect_leaf_rects(
    node: &PaneNode,
    rect: ratatui::layout::Rect,
) -> Vec<(PaneId, ratatui::layout::Rect)> {
    match node {
        PaneNode::Pane(pane) => vec![(pane.id, rect)],
        PaneNode::Container { dir, children, .. } => {
            let n = children.len().max(1);
            let sizes: Vec<u32> = children
                .iter()
                .map(|c| {
                    let s = match c {
                        PaneNode::Pane(p) => p.size,
                        PaneNode::Container { size, .. } => *size,
                    };
                    if s == 0 { 1 } else { s as u32 }
                })
                .collect();
            let total: u32 = sizes.iter().sum::<u32>().max(1);

            let mut constraints = Vec::with_capacity(n * 2);
            for (i, &sz) in sizes.iter().enumerate() {
                constraints.push(Constraint::Ratio(sz, total));
                if i + 1 < n {
                    constraints.push(Constraint::Length(1));
                }
            }

            let areas = match dir {
                SplitDir::Vertical => Layout::horizontal(constraints).split(rect),
                SplitDir::Horizontal => Layout::vertical(constraints).split(rect),
            };

            let mut result = Vec::new();
            for (i, child) in children.iter().enumerate() {
                result.extend(collect_leaf_rects(child, areas[i * 2]));
            }
            result
        }
    }
}

pub async fn register_default_chunks(
    chunks: ResMut<Chunks>,
    window: Res<WindowState>,
    layout: Res<LayoutConfig>,
    split: ResMut<SplitState>,
) {
    get!(mut chunks, window, layout, mut split);

    let size = window.size();

    let [main_area, statusline] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(layout.statusline_height),
    ])
    .areas(size);

    chunks.register_chunk::<StatuslineChunk>(0, statusline);

    let leaf_rects = collect_leaf_rects(&split.root, main_area);
    split.leaf_rects = leaf_rects.clone();

    for (idx, (_, pane_full_rect)) in leaf_rects.iter().enumerate() {
        let [bufferline_rect, content_rect] = Layout::vertical([
            Constraint::Length(layout.bufferline_height),
            Constraint::Fill(1),
        ])
        .areas(*pane_full_rect);

        let [gutter_rect, _pad, buffer_rect] = Layout::horizontal([
            Constraint::Length(layout.gutter_width),
            Constraint::Length(layout.gutter_pad),
            Constraint::Fill(1),
        ])
        .areas(content_rect);

        chunks.register_indexed_chunk::<BufferlineChunk>(idx, 0, bufferline_rect);
        chunks.register_indexed_chunk::<BufferGutterChunk>(idx, 0, gutter_rect);
        chunks.register_indexed_chunk::<BufferChunk>(idx, 0, buffer_rect);
    }

    // Register the focused pane's named chunks for backward compatibility
    if let Some((_, focused_full_rect)) = leaf_rects.iter().find(|(id, _)| *id == split.focused_id)
    {
        let [focused_bufferline, focused_content] = Layout::vertical([
            Constraint::Length(layout.bufferline_height),
            Constraint::Fill(1),
        ])
        .areas(*focused_full_rect);

        let [focused_gutter, _pad2, focused_buffer] = Layout::horizontal([
            Constraint::Length(layout.gutter_width),
            Constraint::Length(layout.gutter_pad),
            Constraint::Fill(1),
        ])
        .areas(focused_content);

        chunks.register_chunk::<BufferlineChunk>(0, focused_bufferline);
        chunks.register_chunk::<BufferGutterChunk>(0, focused_gutter);
        chunks.register_chunk::<BufferChunk>(0, focused_buffer);
    }
}

pub async fn render_chunks(chunks: Res<Chunks>, window: ResMut<WindowState>) {
    get!(chunks, mut window);

    let mut best_cursor: Option<(usize, u16, u16, CursorShape)> = None;

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

    state.lock_state::<Chunks>().await.clear();

    state.hook(hooks::ChunkRegister).call().await;

    let filetype = {
        let bufs = state.lock_state::<Buffers>().await;
        bufs.cur_buffer_as::<TextBuffer>()
            .await
            .map(|tb| tb.ext.clone())
            .unwrap_or_default()
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
            // Check if booster has saved a custom config path in kerbin-info.json
            let booster_path = dirs::home_dir()
                .map(|h| h.join(".kerbin").join("kerbin-info.json"))
                .filter(|p| p.exists())
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v["config_path"].as_str().map(|s| s.to_string()));

            if let Some(path) = booster_path {
                path
            } else if let Ok(home) = std::env::var("XDG_CONFIG_HOME") {
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
        .set_template("cfg_folder", &config_path);

    resolver_engine_mut()
        .await
        .set_template("session", session_id.to_string());

    init_log();

    let terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste).ok();

    // Initialize terminal
    let (command_sender, mut command_receiver) = unbounded_channel();

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
        commands.register::<DebugCommand>();
        commands.register::<IfCommand>();

        commands.register::<AutoPairsCommand>();
        commands.register::<SplitCommand>();
    }

    {
        let mut reg = state.lock_state::<IfCheckRegistry>().await;
        reg.register("mode", |tokens| {
            let c = tokens.first().and_then(|t| {
                if let Token::Word(w) = t {
                    w.chars().next()
                } else {
                    None
                }
            })?;
            Some(Box::new(ModeExistsCheck(c)))
        });
        reg.register("template", |tokens| {
            let name = tokens.first().and_then(|t| {
                if let Token::Word(w) = t {
                    Some(w.clone())
                } else {
                    None
                }
            })?;
            Some(Box::new(TemplateExistsCheck(name)))
        });
        reg.register("text", |tokens| {
            let text = tokens
                .iter()
                .filter_map(|t| {
                    if let Token::Word(w) = t {
                        Some(w.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Some(Box::new(TextExistsCheck(text)))
        });
    }

    config::init(&mut state).await;

    let kb_path = PathBuf::from(format!("{config_path}/init.kb"));
    let errors = load_kb(&kb_path, &mut state).await;
    *state.lock_state::<ConfigErrors>().await = ConfigErrors(errors.clone());
    if !errors.is_empty() {
        state.lock_state::<LogSender>().await.critical(
            "core::config_load",
            format!(
                "{} config error(s) on startup — run `config-errors` to review",
                errors.len()
            ),
        );
    }

    let (framerate, disable_auto_pairs) = {
        let cfg = state.lock_state::<CoreConfig>().await;
        (cfg.framerate, cfg.disable_auto_pairs)
    };
    let ms_per_frame = 1000 / framerate;

    if !disable_auto_pairs {
        state
            .lock_state::<CommandInterceptorRegistry>()
            .await
            .on_command_named::<BufferCommand>("core::auto_pairs", 0, |cmd, state| {
                Box::pin(auto_pairs_intercept(cmd, state))
            });
    }

    // Default mouse bindings (overridable via `mouse-bind` in config)
    {
        let mut mb = state.lock_state::<MouseBindings>().await;
        mb.bindings.insert(
            MouseTrigger::LeftDown,
            vec!["goto %mouse_col %mouse_line".to_string()],
        );
        mb.bindings
            .insert(MouseTrigger::ScrollUp, vec!["ml -3".to_string()]);
        mb.bindings
            .insert(MouseTrigger::ScrollDown, vec!["ml 3".to_string()]);
    }

    state
        .on_hook(hooks::ChunkRegister)
        .system_named("core::layout", register_default_chunks)
        .system_named(
            "core::command_palette_chunks",
            register_command_palette_chunks,
        )
        .system_named("core::log_chunk", register_log_chunk)
        .system_named("core::help_menu_chunk", register_help_menu_chunk);

    state
        .on_hook(hooks::Update)
        .system_named("core::update_debounce", update_debounce)
        .system_named("core::handle_inputs", handle_inputs)
        .system_named("core::handle_mouse_events", handle_mouse_events)
        .system_named(
            "core::update_palette_suggestions",
            update_palette_suggestions,
        )
        .system_named(
            "core::render_cursors_and_selections",
            render_cursors_and_selections,
        );

    state
        .on_hook(hooks::PostUpdate)
        .system_named("core::post_update_buffer", post_update_buffer)
        .system_named("core::update_tab_width_template", update_tab_width_template);

    state
        .on_hook(hooks::PreLines)
        .system_named(
            "core::update_buffer_horizontal_scroll",
            update_buffer_horizontal_scroll,
        )
        .system_named(
            "core::update_buffer_vertical_scroll",
            update_buffer_vertical_scroll,
        )
        .system_named("core::update_bufferline_scroll", update_bufferline_scroll);

    state
        .on_hook(hooks::Render)
        .system_named("core::render_statusline", render_statusline)
        .system_named("core::render_command_palette", render_command_palette)
        .system_named("core::render_help_menu", render_help_menu)
        .system_named("core::render_bufferline", render_bufferline)
        .system_named("core::render_log", render_log)
        .system_named("core::render_buffer", render_buffer_default)
        .system_named("core::render_splits", render_splits);

    state
        .on_hook(hooks::UpdateCleanup)
        .system_named("core::cleanup_buffers", cleanup_buffers);

    state
        .on_hook(hooks::RenderChunks)
        .system_named("core::render_chunks", render_chunks);

    state.hook(hooks::PostInit).call().await;

    for file in args.files {
        let path = file.to_string_lossy().to_string();
        let default_tab_unit = state.lock_state::<CoreConfig>().await.default_tab_unit;
        state
            .lock_state::<Buffers>()
            .await
            .open(path, default_tab_unit)
            .await
            .ok();
    }

    loop {
        let frame_start = tokio::time::Instant::now();

        while let Ok(cmd) = command_receiver.try_recv() {
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
                Some(cmd) = command_receiver.recv() => {
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
