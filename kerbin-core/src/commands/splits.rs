use std::str::FromStr;

use kerbin_macros::Command;
use kerbin_state_machine::State;

use crate::*;

/// The axis direction for a resize command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitResizeDir {
    Vertical,
    Horizontal,
}

impl FromStr for SplitResizeDir {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "v" => Ok(SplitResizeDir::Vertical),
            "h" => Ok(SplitResizeDir::Horizontal),
            other => Err(format!("expected 'v' or 'h', got '{}'", other)),
        }
    }
}

/// Returns the global buffer index that the given pane is currently displaying.
///
/// - `unique_buffers = false` (shared mode): `pane.selected_local` IS the global index.
/// - `unique_buffers = true`: `pane.buffer_indices[pane.selected_local]` is the global index.
fn pane_global_idx(split: &SplitState, pane: &SplitPane) -> Option<usize> {
    if !split.unique_buffers {
        Some(pane.selected_local)
    } else {
        pane.buffer_indices.get(pane.selected_local).copied()
    }
}

async fn apply_buf_focus(state: &mut State, buf_idx: Option<usize>) {
    if let Some(idx) = buf_idx {
        state.lock_state::<Buffers>().await.set_selected_buffer(idx);
    }
}

async fn focus_in_dir(state: &mut State, dx: i16, dy: i16) -> bool {
    let buf_idx = {
        let mut split = state.lock_state::<SplitState>().await;
        split.focus_in_direction(dx, dy);
        let focused_id = split.focused_id;
        split
            .leaves()
            .iter()
            .find(|p| p.id == focused_id)
            .and_then(|p| pane_global_idx(&split, p))
    };
    if let Some(idx) = buf_idx {
        state.lock_state::<Buffers>().await.set_selected_buffer(idx);
    }
    true
}

#[derive(Clone, Debug, Command)]
pub enum SplitCommand {
    #[command(name = "split_v")]
    /// Splits the focused pane vertically (side by side)
    SplitVertical,

    #[command(name = "split_h")]
    /// Splits the focused pane horizontally (stacked)
    SplitHorizontal,

    #[command(drop_ident, name = "split_focus")]
    /// Focuses the split pane at the given leaf index
    FocusPane(usize),

    #[command(name = "split_next")]
    /// Focuses the next pane (wraps around)
    FocusNext,

    #[command(name = "split_prev")]
    /// Focuses the previous pane (wraps around)
    FocusPrev,

    #[command(name = "split_left")]
    /// Focuses the nearest pane to the left
    FocusLeft,

    #[command(name = "split_right")]
    /// Focuses the nearest pane to the right
    FocusRight,

    #[command(name = "split_up")]
    /// Focuses the nearest pane above
    FocusUp,

    #[command(name = "split_down")]
    /// Focuses the nearest pane below
    FocusDown,

    #[command(name = "split_close")]
    /// Closes the current pane; its buffers merge into the adjacent pane
    CloseSplit,

    #[command(drop_ident, name = "split_resize")]
    /// Resizes the focused pane. dir: "v" = width, "h" = height. No-op if wrong axis.
    ResizeSplit {
        #[command(type_name = "v|h")]
        dir: SplitResizeDir,
        amount: i16,
    },

    #[command(name = "drop_splits")]
    /// Closes all panes except the focused one and merges all buffer lists into it
    DropSplits,
}

#[async_trait::async_trait]
impl Command for SplitCommand {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            SplitCommand::SplitVertical | SplitCommand::SplitHorizontal => {
                let dir = if matches!(self, SplitCommand::SplitVertical) {
                    SplitDir::Vertical
                } else {
                    SplitDir::Horizontal
                };

                let buf_idx = {
                    let mut split = state.lock_state::<SplitState>().await;
                    split.split_focused(dir);
                    let focused_id = split.focused_id;
                    split
                        .leaves()
                        .iter()
                        .find(|p| p.id == focused_id)
                        .and_then(|p| pane_global_idx(&split, p))
                };
                apply_buf_focus(state, buf_idx).await;
                true
            }

            SplitCommand::FocusPane(n) => {
                let buf_idx = {
                    let mut split = state.lock_state::<SplitState>().await;
                    let (id, buf_idx) = {
                        let leaves = split.leaves();
                        if let Some(pane) = leaves.get(*n) {
                            (pane.id, pane_global_idx(&split, pane))
                        } else {
                            return false;
                        }
                    };
                    split.focused_id = id;
                    buf_idx
                };
                apply_buf_focus(state, buf_idx).await;
                true
            }

            SplitCommand::FocusNext => {
                let buf_idx = {
                    let mut split = state.lock_state::<SplitState>().await;
                    let count = split.pane_count();
                    if count == 0 {
                        return false;
                    }
                    let cur_idx = split.focused_leaf_idx().unwrap_or(0);
                    let new_idx = (cur_idx + 1) % count;
                    let (id, buf_idx) = {
                        let leaves = split.leaves();
                        let pane = &leaves[new_idx];
                        (pane.id, pane_global_idx(&split, pane))
                    };
                    split.focused_id = id;
                    buf_idx
                };
                apply_buf_focus(state, buf_idx).await;
                true
            }

            SplitCommand::FocusPrev => {
                let buf_idx = {
                    let mut split = state.lock_state::<SplitState>().await;
                    let count = split.pane_count();
                    if count == 0 {
                        return false;
                    }
                    let cur_idx = split.focused_leaf_idx().unwrap_or(0);
                    let new_idx = (cur_idx + count - 1) % count;
                    let (id, buf_idx) = {
                        let leaves = split.leaves();
                        let pane = &leaves[new_idx];
                        (pane.id, pane_global_idx(&split, pane))
                    };
                    split.focused_id = id;
                    buf_idx
                };
                apply_buf_focus(state, buf_idx).await;
                true
            }

            SplitCommand::FocusLeft => focus_in_dir(state, -1, 0).await,
            SplitCommand::FocusRight => focus_in_dir(state, 1, 0).await,
            SplitCommand::FocusUp => focus_in_dir(state, 0, -1).await,
            SplitCommand::FocusDown => focus_in_dir(state, 0, 1).await,

            SplitCommand::CloseSplit => {
                let buf_idx = {
                    let mut split = state.lock_state::<SplitState>().await;
                    let removed = match split.close_focused() {
                        Some(r) => r,
                        None => return false,
                    };

                    if split.unique_buffers
                        && let Some(focused_pane) = split.focused_pane_mut()
                    {
                        for idx in removed.buffer_indices {
                            if !focused_pane.buffer_indices.contains(&idx) {
                                focused_pane.buffer_indices.push(idx);
                            }
                        }
                    }

                    let focused_id = split.focused_id;
                    split
                        .leaves()
                        .iter()
                        .find(|p| p.id == focused_id)
                        .and_then(|p| pane_global_idx(&split, p))
                };
                apply_buf_focus(state, buf_idx).await;
                true
            }

            SplitCommand::ResizeSplit { dir, amount } => {
                let mut split = state.lock_state::<SplitState>().await;
                let focused_id = split.focused_id;
                let parent_dir = split.root.find_parent_dir(focused_id);

                let axis_matches = matches!(
                    (&parent_dir, dir),
                    (Some(SplitDir::Vertical), SplitResizeDir::Vertical)
                        | (Some(SplitDir::Horizontal), SplitResizeDir::Horizontal)
                );
                if !axis_matches {
                    return false;
                }

                if let Some(size) = split.root.find_pane_size_mut(focused_id) {
                    *size = (*size as i16 + amount).max(1) as u16;
                }
                true
            }

            SplitCommand::DropSplits => {
                let buf_idx = {
                    let unique_buffers = state.lock_state::<SplitState>().await.unique_buffers;
                    let merged: Vec<usize> = if !unique_buffers {
                        let buffers = state.lock_state::<Buffers>().await;
                        (0..buffers.buffers.len()).collect()
                    } else {
                        let split = state.lock_state::<SplitState>().await;
                        let mut v: Vec<usize> = Vec::new();
                        for pane in split.leaves() {
                            for &idx in &pane.buffer_indices {
                                if !v.contains(&idx) {
                                    v.push(idx);
                                }
                            }
                        }
                        v
                    };

                    let mut split = state.lock_state::<SplitState>().await;

                    let selected_local = split
                        .focused_pane()
                        .map(|p| p.selected_local)
                        .unwrap_or(0)
                        .min(merged.len().saturating_sub(1));

                    let new_id = split.alloc_id();
                    split.root = PaneNode::Pane(SplitPane {
                        id: new_id,
                        buffer_indices: merged,
                        selected_local,
                        size: 0,
                        tab_scroll: 0,
                    });
                    split.focused_id = new_id;

                    split.focused_pane().and_then(|p| pane_global_idx(&split, p))
                };
                apply_buf_focus(state, buf_idx).await;
                true
            }
        }
    }
}
