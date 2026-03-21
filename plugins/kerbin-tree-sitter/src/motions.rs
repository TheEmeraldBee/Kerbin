use kerbin_core::*;

use crate::state::TreeSitterState;

#[derive(Command)]
pub enum TreeSitterMotion {
    #[command(drop_ident, name = "ts_select_node", name = "tssn")]
    /// Select the smallest named AST node covering the cursor.
    /// If the selection already matches that node, expands to parent.
    SelectNode {
        #[command(flag)]
        extend: bool,
    },

    #[command(drop_ident, name = "ts_parent_node", name = "tspn")]
    /// Expand selection to the parent AST node.
    ParentNode {
        #[command(flag)]
        extend: bool,
    },

    #[command(drop_ident, name = "ts_next_sibling", name = "tsns")]
    /// Move to the next named sibling node.
    NextSibling {
        #[command(flag)]
        extend: bool,
    },

    #[command(drop_ident, name = "ts_prev_sibling", name = "tsps")]
    /// Move to the previous named sibling node.
    PrevSibling {
        #[command(flag)]
        extend: bool,
    },
}

#[async_trait::async_trait]
impl Command for TreeSitterMotion {
    async fn apply(&self, state: &mut State) -> bool {
        match self {
            Self::SelectNode { extend } => ts_select_node(state, *extend).await,
            Self::ParentNode { extend } => ts_parent_node(state, *extend).await,
            Self::NextSibling { extend } => ts_next_sibling(state, *extend).await,
            Self::PrevSibling { extend } => ts_prev_sibling(state, *extend).await,
        }
        true
    }
}

fn apply_selection(buf: &mut TextBuffer, range: std::ops::Range<usize>, extend: bool) {
    let new_start = range.start;
    let new_end = range.end.saturating_sub(1);
    if extend {
        let existing = buf.primary_cursor().sel().clone();
        let start = (*existing.start()).min(new_start);
        let end = (*existing.end()).max(new_end);
        buf.primary_cursor_mut().set_sel(start..=end);
    } else {
        buf.primary_cursor_mut().set_sel(new_start..=new_end);
        buf.primary_cursor_mut().set_at_start(false);
    }
}

async fn ts_select_node(state: &mut State, extend: bool) {
    let mut buffers = state.lock_state::<Buffers>().await;
    let mut buf = buffers.cur_buffer_mut().await;

    let sel_start = *buf.primary_cursor().sel().start();
    let sel_end = *buf.primary_cursor().sel().end();

    let final_range = {
        let Some(ts_state) = buf.get_state_mut::<TreeSitterState>().await else {
            return;
        };
        let Some(tree) = &ts_state.tree else {
            return;
        };

        let root = tree.root_node();
        let Some(node) = root.named_descendant_for_byte_range(sel_start, sel_end + 1) else {
            return;
        };
        let node_range = node.byte_range();

        if node_range.start == sel_start && node_range.end.saturating_sub(1) == sel_end {
            if let Some(parent) = node.parent() {
                parent.byte_range()
            } else {
                node_range
            }
        } else {
            node_range
        }
    };

    apply_selection(&mut buf, final_range, extend);
}

async fn ts_parent_node(state: &mut State, extend: bool) {
    let mut buffers = state.lock_state::<Buffers>().await;
    let mut buf = buffers.cur_buffer_mut().await;

    let sel_start = *buf.primary_cursor().sel().start();
    let sel_end = *buf.primary_cursor().sel().end();

    let final_range = {
        let Some(ts_state) = buf.get_state_mut::<TreeSitterState>().await else {
            return;
        };
        let Some(tree) = &ts_state.tree else {
            return;
        };

        let root = tree.root_node();
        let Some(node) = root.named_descendant_for_byte_range(sel_start, sel_end + 1) else {
            return;
        };

        let mut current = node;
        loop {
            let Some(parent) = current.parent() else {
                break current.byte_range();
            };
            let parent_range = parent.byte_range();
            let current_range = current.byte_range();
            if parent_range.start < current_range.start || parent_range.end > current_range.end {
                break parent_range;
            }
            current = parent;
        }
    };

    apply_selection(&mut buf, final_range, extend);
}

async fn ts_next_sibling(state: &mut State, extend: bool) {
    let mut buffers = state.lock_state::<Buffers>().await;
    let mut buf = buffers.cur_buffer_mut().await;

    let sel_start = *buf.primary_cursor().sel().start();
    let sel_end = *buf.primary_cursor().sel().end();

    let final_range = {
        let Some(ts_state) = buf.get_state_mut::<TreeSitterState>().await else {
            return;
        };
        let Some(tree) = &ts_state.tree else {
            return;
        };

        let root = tree.root_node();
        let Some(node) = root.named_descendant_for_byte_range(sel_start, sel_end + 1) else {
            return;
        };

        let Some(sibling) = node.next_named_sibling() else {
            return;
        };
        sibling.byte_range()
    };

    apply_selection(&mut buf, final_range, extend);
}

async fn ts_prev_sibling(state: &mut State, extend: bool) {
    let mut buffers = state.lock_state::<Buffers>().await;
    let mut buf = buffers.cur_buffer_mut().await;

    let sel_start = *buf.primary_cursor().sel().start();
    let sel_end = *buf.primary_cursor().sel().end();

    let final_range = {
        let Some(ts_state) = buf.get_state_mut::<TreeSitterState>().await else {
            return;
        };
        let Some(tree) = &ts_state.tree else {
            return;
        };

        let root = tree.root_node();
        let Some(node) = root.named_descendant_for_byte_range(sel_start, sel_end + 1) else {
            return;
        };

        let Some(sibling) = node.prev_named_sibling() else {
            return;
        };
        sibling.byte_range()
    };

    apply_selection(&mut buf, final_range, extend);
}
