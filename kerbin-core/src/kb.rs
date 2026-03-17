use crate::*;
use std::error::Error;
use std::path::Path;

/// Load and execute a `.kb` config file against `state`.
///
/// Lines are tokenized and dispatched through the command registry.
/// The `ConfigDir` state is temporarily updated to the file's directory
/// so that nested `source` commands resolve paths correctly.
pub async fn load_kb(path: &Path, state: &mut State) -> Result<(), Box<dyn Error>> {
    let content = std::fs::read_to_string(path)?;
    let base_dir = path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();

    // Save and update ConfigDir to this file's directory
    let old_dir = {
        let mut cfg_dir = state.lock_state::<ConfigDir>().await;
        let old = cfg_dir.0.clone();
        cfg_dir.0 = base_dir;
        old
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let tokens = match tokenize(line) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("kb parse error in '{}' on line {:?}: {}", path.display(), line, e);
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
            tracing::warn!("kb [{}] unrecognized command on line: {:?}", path.display(), line);
        }
    }

    // Restore previous ConfigDir
    state.lock_state::<ConfigDir>().await.0 = old_dir;

    Ok(())
}
