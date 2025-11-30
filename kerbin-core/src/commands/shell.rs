use crate::*;
use ascii_forge::window::crossterm::cursor::Hide;
use ascii_forge::window::crossterm::execute;
use ascii_forge::window::crossterm::terminal::{
    DisableLineWrap, EnterAlternateScreen, enable_raw_mode,
};
use ascii_forge::window::{EnableFocusChange, EnableMouseCapture};
use std::process::Stdio;

fn execute_parser(val: &[String]) -> Result<Box<dyn Command>, String> {
    if val.len() == 1 {
        return Err("Expected at least 1 argument".to_string());
    }
    Ok(Box::new(ShellCommand::Execute(val[1..].to_vec())))
}

fn spawn_parser(val: &[String]) -> Result<Box<dyn Command>, String> {
    if val.len() == 1 {
        return Err("Expected at least 1 argument".to_string());
    }
    Ok(Box::new(ShellCommand::Spawn(val[1..].to_vec())))
}

fn in_place_parser(val: &[String]) -> Result<Box<dyn Command>, String> {
    if val.len() == 1 {
        return Err("Expected at least 1 argument".to_string());
    }
    Ok(Box::new(ShellCommand::InPlace(val[1..].to_vec())))
}

#[derive(Debug, Clone, Command)]
pub enum ShellCommand {
    #[command(parser = "execute_parser", drop_ident, name = "shell", name = "sh")]
    /// Executes a shell command, freezing until it is executed
    /// Should probably be ignored in favor of spawn or in_place
    Execute(#[command(name = "cmd", type_name = "rest")] Vec<String>),
    #[command(
        parser = "spawn_parser",
        drop_ident,
        name = "shell_spawn",
        name = "shsp"
    )]

    /// Spawns a shell command in the background
    Spawn(#[command(name = "cmd", type_name = "rest")] Vec<String>),

    /// Spawns a shell command, replacing stdin with this
    /// Reapply's window when rendering app again
    ///
    /// Results in pausing the editor until command is finished
    #[command(
        parser = "in_place_parser",
        drop_ident,
        name = "shell_in_place",
        name = "ship"
    )]
    InPlace(#[command(name = "cmd", type_name = "rest")] Vec<String>),
}

#[async_trait::async_trait]
impl Command for ShellCommand {
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
            Self::InPlace(args) => {
                let mut window = state.lock_state::<WindowState>().await;
                window.restore().unwrap();

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

                enable_raw_mode().unwrap();
                execute!(
                    window.io(),
                    EnterAlternateScreen,
                    EnableMouseCapture,
                    EnableFocusChange,
                    Hide,
                    DisableLineWrap,
                )
                .unwrap();

                window.buffer_mut().fill(" ");
                window.swap_buffers();

                res
            }
        }
    }
}
