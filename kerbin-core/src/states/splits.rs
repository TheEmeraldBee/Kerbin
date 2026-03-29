use ratatui::layout::Rect;

use crate::*;

/// The axis along which panes are split
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitDir {
    Vertical,
    Horizontal,
}

/// Unique identifier for a pane
pub type PaneId = usize;

/// A single pane in the split layout
pub struct SplitPane {
    pub id: PaneId,
    pub buffer_indices: Vec<usize>,
    pub selected_local: usize,
    pub size: u16,
    pub tab_scroll: usize,
}

/// A node in the recursive pane tree
pub enum PaneNode {
    Pane(SplitPane),
    Container {
        dir: SplitDir,
        size: u16,
        children: Vec<PaneNode>,
    },
}

impl PaneNode {
    pub fn collect_leaves<'a>(&'a self, out: &mut Vec<&'a SplitPane>) {
        match self {
            PaneNode::Pane(p) => out.push(p),
            PaneNode::Container { children, .. } => {
                for child in children {
                    child.collect_leaves(out);
                }
            }
        }
    }

    pub fn collect_leaves_mut<'a>(&'a mut self, out: &mut Vec<&'a mut SplitPane>) {
        match self {
            PaneNode::Pane(p) => out.push(p),
            PaneNode::Container { children, .. } => {
                for child in children {
                    child.collect_leaves_mut(out);
                }
            }
        }
    }

    pub fn find_pane(&self, id: PaneId) -> Option<&SplitPane> {
        match self {
            PaneNode::Pane(p) if p.id == id => Some(p),
            PaneNode::Pane(_) => None,
            PaneNode::Container { children, .. } => {
                for child in children {
                    if let Some(p) = child.find_pane(id) {
                        return Some(p);
                    }
                }
                None
            }
        }
    }

    pub fn find_pane_mut(&mut self, id: PaneId) -> Option<&mut SplitPane> {
        match self {
            PaneNode::Pane(p) if p.id == id => Some(p),
            PaneNode::Pane(_) => None,
            PaneNode::Container { children, .. } => {
                for child in children {
                    if let Some(p) = child.find_pane_mut(id) {
                        return Some(p);
                    }
                }
                None
            }
        }
    }

    pub fn find_pane_size_mut(&mut self, id: PaneId) -> Option<&mut u16> {
        match self {
            PaneNode::Pane(p) if p.id == id => Some(&mut p.size),
            PaneNode::Pane(_) => None,
            PaneNode::Container { children, .. } => {
                for child in children {
                    if let Some(s) = child.find_pane_size_mut(id) {
                        return Some(s);
                    }
                }
                None
            }
        }
    }

    pub fn find_parent_dir(&self, id: PaneId) -> Option<SplitDir> {
        match self {
            PaneNode::Pane(_) => None,
            PaneNode::Container { dir, children, .. } => {
                for child in children {
                    if matches!(child, PaneNode::Pane(p) if p.id == id) {
                        return Some(dir.clone());
                    }
                    if let Some(d) = child.find_parent_dir(id) {
                        return Some(d);
                    }
                }
                None
            }
        }
    }

    pub fn remove_pane(&mut self, id: PaneId) -> Option<SplitPane> {
        match self {
            PaneNode::Pane(_) => None,
            PaneNode::Container { children, .. } => {
                for i in 0..children.len() {
                    if matches!(&children[i], PaneNode::Pane(p) if p.id == id)
                        && let PaneNode::Pane(removed) = children.remove(i)
                    {
                        return Some(removed);
                    }
                }
                for child in children {
                    if let Some(p) = child.remove_pane(id) {
                        return Some(p);
                    }
                }
                None
            }
        }
    }

    pub fn insert_after_pane(
        &mut self,
        target_id: PaneId,
        new_node: PaneNode,
        new_dir: SplitDir,
    ) -> bool {
        match self {
            PaneNode::Pane(_) => false,
            PaneNode::Container { dir, children, .. } => {
                let direct_idx = children
                    .iter()
                    .position(|c| matches!(c, PaneNode::Pane(p) if p.id == target_id));

                if let Some(i) = direct_idx {
                    if *dir == new_dir {
                        children.insert(i + 1, new_node);
                    } else {
                        let target = children.remove(i);
                        let target_size = match &target {
                            PaneNode::Pane(p) => p.size,
                            PaneNode::Container { size, .. } => *size,
                        };
                        let wrapper = PaneNode::Container {
                            dir: new_dir,
                            size: target_size,
                            children: vec![target, new_node],
                        };
                        children.insert(i, wrapper);
                    }
                    return true;
                }

                let recurse_idx = children
                    .iter()
                    .position(|c| c.find_pane(target_id).is_some());
                if let Some(idx) = recurse_idx {
                    return children[idx].insert_after_pane(target_id, new_node, new_dir);
                }

                false
            }
        }
    }

    pub fn collapse(self) -> PaneNode {
        match self {
            PaneNode::Pane(_) => self,
            PaneNode::Container {
                dir,
                size,
                children,
            } => {
                let collapsed: Vec<PaneNode> = children.into_iter().map(|c| c.collapse()).collect();
                if collapsed.len() == 1 {
                    let mut child = collapsed.into_iter().next().unwrap();
                    match &mut child {
                        PaneNode::Pane(p) => p.size = size,
                        PaneNode::Container { size: s, .. } => *s = size,
                    }
                    child
                } else {
                    PaneNode::Container {
                        dir,
                        size,
                        children: collapsed,
                    }
                }
            }
        }
    }
}

fn placeholder_pane() -> PaneNode {
    PaneNode::Pane(SplitPane {
        id: PaneId::MAX,
        buffer_indices: vec![],
        selected_local: 0,
        size: 0,
        tab_scroll: 0,
    })
}

/// State tracking the split-window layout
#[derive(State)]
pub struct SplitState {
    pub root: PaneNode,
    pub focused_id: PaneId,
    pub next_id: PaneId,

    pub unique_buffers: bool,
    pub leaf_rects: Vec<(PaneId, Rect)>,
}

impl Default for SplitState {
    fn default() -> Self {
        Self {
            root: PaneNode::Pane(SplitPane {
                id: 0,
                buffer_indices: vec![],
                selected_local: 0,
                size: 0,
                tab_scroll: 0,
            }),
            focused_id: 0,
            next_id: 1,
            unique_buffers: false,
            leaf_rects: Vec::new(),
        }
    }
}

impl SplitState {
    pub fn alloc_id(&mut self) -> PaneId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn leaves(&self) -> Vec<&SplitPane> {
        let mut out = Vec::new();
        self.root.collect_leaves(&mut out);
        out
    }

    pub fn leaves_mut(&mut self) -> Vec<&mut SplitPane> {
        let mut out = Vec::new();
        self.root.collect_leaves_mut(&mut out);
        out
    }

    pub fn pane_count(&self) -> usize {
        self.leaves().len()
    }

    pub fn focused_pane(&self) -> Option<&SplitPane> {
        self.root.find_pane(self.focused_id)
    }

    pub fn focused_pane_mut(&mut self) -> Option<&mut SplitPane> {
        self.root.find_pane_mut(self.focused_id)
    }

    pub fn focused_leaf_idx(&self) -> Option<usize> {
        let id = self.focused_id;
        self.leaves().iter().position(|p| p.id == id)
    }

    /// Splits the focused pane and returns the new pane's id.
    pub fn split_focused(&mut self, dir: SplitDir) -> PaneId {
        let new_id = self.alloc_id();
        let focused_id = self.focused_id;

        let new_pane = {
            let focused = self.focused_pane();
            SplitPane {
                id: new_id,
                buffer_indices: focused
                    .map(|p| p.buffer_indices.clone())
                    .unwrap_or_default(),
                selected_local: focused.map(|p| p.selected_local).unwrap_or(0),
                size: 0,
                tab_scroll: 0,
            }
        };

        if matches!(&self.root, PaneNode::Pane(p) if p.id == focused_id) {
            let old_root = std::mem::replace(&mut self.root, placeholder_pane());
            self.root = PaneNode::Container {
                dir,
                size: 0,
                children: vec![old_root, PaneNode::Pane(new_pane)],
            };
        } else {
            self.root
                .insert_after_pane(focused_id, PaneNode::Pane(new_pane), dir);
        }

        self.focused_id = new_id;
        new_id
    }

    pub fn close_focused(&mut self) -> Option<SplitPane> {
        if self.pane_count() <= 1 {
            return None;
        }

        let old_idx = self.focused_leaf_idx()?;
        let removed = self.root.remove_pane(self.focused_id)?;

        let old_root = std::mem::replace(&mut self.root, placeholder_pane());
        self.root = old_root.collapse();

        let leaves = self.leaves();
        let new_idx = old_idx.min(leaves.len().saturating_sub(1));
        if let Some(pane) = leaves.get(new_idx) {
            self.focused_id = pane.id;
        }

        Some(removed)
    }

    pub fn focus_in_direction(&mut self, dx: i16, dy: i16) {
        let focused_rect = self
            .leaf_rects
            .iter()
            .find(|(id, _)| *id == self.focused_id)
            .map(|(_, r)| *r);

        let Some(focused_rect) = focused_rect else {
            return;
        };

        let cx = focused_rect.x as f32 + focused_rect.width as f32 / 2.0;
        let cy = focused_rect.y as f32 + focused_rect.height as f32 / 2.0;

        let best = self
            .leaf_rects
            .iter()
            .filter(|(id, r)| {
                if *id == self.focused_id {
                    return false;
                }
                if dx > 0 {
                    r.x >= focused_rect.x + focused_rect.width
                } else if dx < 0 {
                    r.x + r.width <= focused_rect.x
                } else if dy > 0 {
                    r.y >= focused_rect.y + focused_rect.height
                } else if dy < 0 {
                    r.y + r.height <= focused_rect.y
                } else {
                    false
                }
            })
            .min_by(|(_, a), (_, b)| {
                let acx = a.x as f32 + a.width as f32 / 2.0;
                let acy = a.y as f32 + a.height as f32 / 2.0;
                let bcx = b.x as f32 + b.width as f32 / 2.0;
                let bcy = b.y as f32 + b.height as f32 / 2.0;
                let ad = ((acx - cx).powi(2) + (acy - cy).powi(2)).sqrt();
                let bd = ((bcx - cx).powi(2) + (bcy - cy).powi(2)).sqrt();
                ad.partial_cmp(&bd).unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some((id, _)) = best {
            self.focused_id = *id;
        }
    }
}
