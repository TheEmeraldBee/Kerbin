use crate::*;
use crossterm::{
    cursor::Hide,
    event::{
        DisableMouseCapture, EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{DisableLineWrap, EnterAlternateScreen, enable_raw_mode},
};
use ratatui::widgets::Clear;
use std::process::Stdio;

#[derive(Debug, Clone, Command)]
pub enum ShellCommand {
    #[command(drop_ident, name = "shell", name = "sh")]
    /// Executes a shell command, freezing until it is executed
    /// Should probably be ignored in favor of spawn or in_place
    Execute(#[command(name = "cmd", type_name = "[string]")] Vec<String>),

    #[command(drop_ident, name = "shell_spawn", name = "shsp")]
    /// Spawns a shell command in the background
    Spawn(#[command(name = "cmd", type_name = "[string]")] Vec<String>),

    #[command]
    /// Executes a shell command, running the inner command with the template `out` set
    ///
    /// `pipe` is the shell command to run
    /// `cmd` is the command to run after
    Pipe(
        #[command(flag, name = "pipe", type_name = "[string]")] Vec<String>,
        #[command(flag, name = "cmd", type_name = "[string]")] Vec<Token>,
    ),

    /// Spawns a shell command, replacing stdin with this
    /// Reapplies window when rendering app again
    ///
    /// Results in pausing the editor until command is finished
    #[command(drop_ident, name = "shell_in_place", name = "ship")]
    InPlace(#[command(name = "cmd", type_name = "[string]")] Vec<String>),
}

#[async_trait::async_trait]
impl Command<State> for ShellCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Execute(args) => {
                match std::process::Command::new(&args[0])
                    .args(&args[1..])
                    .output()
                {
                    Ok(_) => true,
                    Err(e) => {
                        tracing::error!("Failed to run command: {e}");
                        false
                    }
                }
            }
            Self::Spawn(args) => {
                match std::process::Command::new(&args[0])
                    .args(&args[1..])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdin(Stdio::piped())
                    .spawn()
                {
                    Ok(_) => true,
                    Err(e) => {
                        tracing::error!("Failed to run command: {e}");
                        false
                    }
                }
            }
            Self::Pipe(pipe, cmd) => {
                let text = match std::process::Command::new(&pipe[0])
                    .args(&pipe[1..])
                    .output()
                {
                    Ok(t) => String::from_utf8_lossy(&t.stdout).to_string(),

                    Err(e) => {
                        tracing::error!("Failed to run command: {e}");
                        return false;
                    }
                };

                resolver_engine_mut().await.set_template("out", text);

                let command = state.lock_state::<CommandRegistry>().await.parse_command(
                    cmd.clone(),
                    true,
                    false,
                    Some(&resolver_engine().await.as_resolver()),
                    true,
                    &*state.lock_state::<CommandPrefixRegistry>().await,
                    &*state.lock_state::<ModeStack>().await,
                );
                if let Some(command) = command {
                    state
                        .lock_state::<CommandSender>()
                        .await
                        .send(command)
                        .unwrap();
                }

                true
            }
            Self::InPlace(args) => {
                // Tear down terminal
                crossterm::terminal::disable_raw_mode().ok();
                execute!(
                    std::io::stdout(),
                    crossterm::terminal::LeaveAlternateScreen,
                    DisableMouseCapture,
                    PopKeyboardEnhancementFlags,
                )
                .ok();

                let res = match std::process::Command::new(&args[0])
                    .args(&args[1..])
                    .status()
                {
                    Ok(_) => true,
                    Err(e) => {
                        tracing::error!("Failed to run command: {e}");
                        false
                    }
                };

                // Restore terminal
                enable_raw_mode().ok();
                execute!(
                    std::io::stdout(),
                    EnterAlternateScreen,
                    EnableMouseCapture,
                    PushKeyboardEnhancementFlags(
                        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
                    ),
                    EnableFocusChange,
                    Hide,
                    DisableLineWrap,
                )
                .ok();

                state
                    .lock_state::<WindowState>()
                    .await
                    .draw(|x| x.render_widget(Clear, x.area()))
                    .expect("terminal should render");

                res
            }
        }
    }
}
