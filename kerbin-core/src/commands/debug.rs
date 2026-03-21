use crate::*;

#[derive(Command)]
pub enum DebugCommand {
    #[command]
    /// Outputs text/templates as raw text to the screen.
    ///
    /// `--level` can be: `low`|`medium`|`high`|`critical`
    /// Defaults to `medium`
    Echo {
        text: Vec<String>,

        #[command(flag)]
        level: Option<String>,
    },
}

#[async_trait::async_trait]
impl Command for DebugCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::Echo { text, level } => {
                let text = text.join(" ");
                let log = state.lock_state::<LogSender>().await;
                if let Some(level) = level {
                    let _ = match level.as_str() {
                        "low" => log.low("echo", text),
                        "medium" => log.medium("echo", text),
                        "high" => log.high("echo", text),
                        "critical" => log.critical("echo", text),
                        _ => log.medium("echo", text),
                    };
                } else {
                    log.medium("echo", text);
                }

                true
            }
        }
    }
}
