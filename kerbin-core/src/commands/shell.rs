use crate::*;
use kerbin_macros::Command;
use kerbin_state_machine::State;
use std::collections::HashMap;
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

/// Performs shell-style variable replacement on a string
/// %var gets replaced with the value from replacements map
/// %%var becomes %var (escaped)
fn replace_variables(input: &str, replacements: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            if chars.peek() == Some(&'%') {
                chars.next();
                result.push('%');
            } else {
                let mut var_name = String::new();
                while let Some(ch) = chars.next_if(|x| x.is_alphanumeric() || *x == '_') {
                    var_name.push(ch);
                }

                if let Some(value) = replacements.get(&var_name) {
                    result.push_str(value);
                } else {
                    result.push('%');
                    result.push_str(&var_name);
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[derive(Debug, Clone, Command)]
pub enum ShellCommand {
    #[command(parser = "execute_parser", drop_ident, name = "shell", name = "sh")]
    /// Executes a shell command, freezing until it is executed
    /// Should probably be ignored in favor of spawn
    Execute(#[command(name = "cmd", type_name = "rest")] Vec<String>),
    #[command(
        parser = "spawn_parser",
        drop_ident,
        name = "shell_spawn",
        name = "shsp"
    )]
    /// Spawns a shell command in the background
    Spawn(#[command(name = "cmd", type_name = "rest")] Vec<String>),
}

#[async_trait::async_trait]
impl Command for ShellCommand {
    async fn apply(&self, state: &mut State) -> bool {
        let uuid = state
            .lock_state::<SessionUuid>()
            .await
            .unwrap()
            .0
            .to_string();
        let config_folder = state.lock_state::<ConfigFolder>().await.unwrap().0.clone();
        let cur_buf = state
            .lock_state::<Buffers>()
            .await
            .unwrap()
            .cur_buffer()
            .await;

        let mut replacements = HashMap::new();
        replacements.insert("session".to_string(), uuid);
        replacements.insert("cfg_folder".to_string(), config_folder);
        replacements.insert("cur_buf".to_string(), cur_buf.path.clone());

        match self {
            Self::Execute(args) => {
                // Apply replacements to all arguments
                let processed_args: Vec<String> = args
                    .iter()
                    .map(|arg| replace_variables(arg, &replacements))
                    .collect();

                match std::process::Command::new(&processed_args[0])
                    .args(&processed_args[1..])
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
                // Apply replacements to all arguments
                let processed_args: Vec<String> = args
                    .iter()
                    .map(|arg| replace_variables(arg, &replacements))
                    .collect();

                match std::process::Command::new(&processed_args[0])
                    .args(&processed_args[1..])
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_variables() {
        let mut replacements = HashMap::new();
        replacements.insert("x".to_string(), "5".to_string());
        replacements.insert("name".to_string(), "test".to_string());

        assert_eq!(replace_variables("hello %x", &replacements), "hello 5");
        assert_eq!(replace_variables("%%x", &replacements), "%x");
        assert_eq!(replace_variables("%x %%x %x", &replacements), "5 %x 5");
        assert_eq!(replace_variables("%name_test", &replacements), "test_test");
        assert_eq!(replace_variables("%unknown", &replacements), "%unknown");
    }
}
