use kerbin_macros::Command;

use crate::*;

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

#[derive(Debug, Clone, Command)]
pub enum ShellCommand {
    #[command(parser = "execute_parser", drop_ident, name = "shell", name = "sh")]
    Execute(#[command(name = "cmd", type_name = "rest")] Vec<String>),

    #[command(
        parser = "spawn_parser",
        drop_ident,
        name = "shell_spawn",
        name = "shsp"
    )]
    Spawn(#[command(name = "cmd", type_name = "rest")] Vec<String>),
}

impl Command for ShellCommand {
    fn apply(&self, _state: std::sync::Arc<State>) -> bool {
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
            Self::Spawn(args) => match std::process::Command::new(&args[0])
                .args(&args[1..])
                .spawn()
            {
                Ok(_) => true,
                Err(e) => {
                    tracing::error!("Failed to run command: {e}");
                    false
                }
            },
        }
    }
}
