use crate::*;
use std::path::{Path, PathBuf};

/// An error encountered while loading a `.kb` config file.
#[derive(Clone)]
pub struct KbLoadError {
    pub path: PathBuf,
    pub line: String,
    pub message: String,
}

/// Load and execute a `.kb` config file against `state`.
///
/// Lines are tokenized and dispatched through the command registry.
/// The `ConfigDir` state is temporarily updated to the file's directory
/// so that nested `source` commands resolve paths correctly.
///
/// Returns a list of errors encountered during loading.
pub async fn load_kb(path: &Path, state: &mut State) -> Vec<KbLoadError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("kb: failed to read '{}': {}", path.display(), e);
            return vec![KbLoadError {
                path: path.to_path_buf(),
                line: String::new(),
                message: e.to_string(),
            }];
        }
    };

    let base_dir = path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();

    // Save and update ConfigDir to this file's directory
    let old_dir = {
        let mut cfg_dir = state.lock_state::<ConfigDir>().await;
        let old = cfg_dir.0.clone();
        cfg_dir.0 = base_dir;
        old
    };

    let mut errors = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let tokens = match tokenize(line) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(
                    "kb parse error in '{}' on line {:?}: {}",
                    path.display(),
                    line,
                    e
                );
                errors.push(KbLoadError {
                    path: path.to_path_buf(),
                    line: line.to_string(),
                    message: e.to_string(),
                });
                continue;
            }
        };

        let command = {
            let registry = state.lock_state::<CommandRegistry>().await;
            let prefix_reg = state.lock_state::<CommandPrefixRegistry>().await;
            let modes = state.lock_state::<ModeStack>().await;
            let cmd = registry.parse_command(tokens, true, true, None, false, &prefix_reg, &modes);
            drop(modes);
            drop(prefix_reg);
            drop(registry);
            cmd
        };

        if let Some(cmd) = command {
            cmd.apply(state).await;
        } else {
            tracing::warn!(
                "kb [{}] unrecognized command on line: {:?}",
                path.display(),
                line
            );
            errors.push(KbLoadError {
                path: path.to_path_buf(),
                line: line.to_string(),
                message: "unrecognized command".to_string(),
            });
        }
    }

    // Restore previous ConfigDir
    state.lock_state::<ConfigDir>().await.0 = old_dir;

    errors
}

/// Reset all config-managed state to defaults, in preparation for reloading `.kb` files.
///
/// Fires the `ResetState` hook so plugins can clear their own config-managed state.
pub async fn reset_config_state(state: &mut State) {
    state.hook(hooks::ResetState).call().await;

    *state.lock_state::<InputState>().await = InputState::default();
    *state.lock_state::<PaletteState>().await = PaletteState::default();
    *state.lock_state::<Theme>().await = Theme::default();
    state.lock_state::<CommandPrefixRegistry>().await.clear();
    *state.lock_state::<CoreConfig>().await = CoreConfig::default();
    *state.lock_state::<DebounceConfig>().await = DebounceConfig::default();
    *state.lock_state::<StatuslineConfig>().await = StatuslineConfig::default();
    state
        .lock_state::<CommandInterceptorRegistry>()
        .await
        .remove_command_interceptor::<BufferCommand>("core::auto_pairs");
}
