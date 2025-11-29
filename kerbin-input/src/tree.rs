use std::sync::Arc;

use ascii_forge::window::{KeyCode, KeyModifiers};
use indexmap::IndexMap;

use crate::{ParseError, ResolvedKeyBind, Resolver, UnresolvedKeyBind};

#[derive(Debug, Clone)]
pub enum KeyItem<A: Clone> {
    /// Stores indices into a Vec of unresolved bindings
    Tree(Vec<UnresolvedKeyBind>, Vec<Arc<KeyItem<A>>>, Option<usize>),
    Leaf(Option<usize>, A),
}

pub struct KeyTree<A: Clone, M: Clone> {
    /// The main key binding tree storage - now stores multiple items per key
    tree: IndexMap<ResolvedKeyBind, Vec<Arc<KeyItem<A>>>>,

    /// Metadata for a item in the tree
    metadata: Vec<M>,

    /// Stack of active tree nodes as we descend
    /// Changed to store Vec index as well
    active_tree: Option<(usize, Arc<KeyItem<A>>, usize)>,

    /// Lazily resolved bindings for the current level only
    /// Maps resolved keys to (vec_index, child_index) pairs
    resolved_cache: Option<IndexMap<ResolvedKeyBind, Vec<(usize, usize)>>>,

    current_sequence: Vec<ResolvedKeyBind>,
}

impl<A: Clone, M: Clone> Default for KeyTree<A, M> {
    fn default() -> Self {
        Self {
            tree: IndexMap::new(),
            metadata: vec![],

            active_tree: None,
            resolved_cache: None,

            current_sequence: vec![],
        }
    }
}

impl<A: Clone, M: Clone> KeyTree<A, M> {
    pub fn set_metadata(
        &mut self,
        resolver: &Resolver,
        bind_sequence: Vec<UnresolvedKeyBind>,
        metadata: M,
    ) -> Result<(), ParseError> {
        if bind_sequence.is_empty() {
            return Err(ParseError::Custom("Empty keybind sequence".into()));
        }

        let first_resolved = resolver.resolve(bind_sequence[0].clone())?;

        let metadata_index = self.metadata.len();
        self.metadata.push(metadata);

        for resolved_key in first_resolved {
            if bind_sequence.len() == 1 {
                if let Some(items) = self.tree.get_mut(&resolved_key) {
                    // Update the most recent (last) item
                    if let Some(item) = items.last_mut() {
                        match Arc::make_mut(item) {
                            KeyItem::Leaf(meta_idx, _) => {
                                *meta_idx = Some(metadata_index);
                            }
                            KeyItem::Tree(_, _, meta_idx) => {
                                *meta_idx = Some(metadata_index);
                            }
                        }
                    }
                } else {
                    return Err(ParseError::Custom("Key not found in tree".into()));
                }
            } else {
                self.set_metadata_sequence(resolved_key, &bind_sequence[1..], metadata_index)?;
            }
        }

        Ok(())
    }

    fn set_metadata_sequence(
        &mut self,
        first_key: ResolvedKeyBind,
        remaining: &[UnresolvedKeyBind],
        metadata_index: usize,
    ) -> Result<(), ParseError> {
        if remaining.is_empty() {
            return Err(ParseError::Custom("Empty remaining sequence".into()));
        }

        let items = self
            .tree
            .get_mut(&first_key)
            .ok_or_else(|| ParseError::Custom("Key not found in tree".into()))?;

        // Update the most recent item
        let existing = items
            .last_mut()
            .ok_or_else(|| ParseError::Custom("No items for key".into()))?;

        let KeyItem::Tree(bindings, children, _) = Arc::make_mut(existing) else {
            return Err(ParseError::Custom("Expected tree node, found leaf".into()));
        };

        let target_bind = &remaining[0];
        let child_idx = bindings
            .iter()
            .position(|bind| bind == target_bind)
            .ok_or_else(|| ParseError::Custom("Key sequence not found".into()))?;

        if remaining.len() == 1 {
            let child = Arc::make_mut(&mut children[child_idx]);
            match child {
                KeyItem::Leaf(meta_idx, _) => {
                    *meta_idx = Some(metadata_index);
                }
                KeyItem::Tree(_, _, meta_idx) => {
                    *meta_idx = Some(metadata_index);
                }
            }
            Ok(())
        } else {
            Self::update_child_metadata(&mut children[child_idx], &remaining[1..], metadata_index)
        }
    }

    fn update_child_metadata(
        child: &mut Arc<KeyItem<A>>,
        remaining: &[UnresolvedKeyBind],
        metadata_index: usize,
    ) -> Result<(), ParseError> {
        if remaining.is_empty() {
            return Err(ParseError::Custom("Empty remaining sequence".into()));
        }

        let child_mut = Arc::make_mut(child);

        let KeyItem::Tree(bindings, children, _) = child_mut else {
            return Err(ParseError::Custom("Expected tree node, found leaf".into()));
        };

        let target_bind = &remaining[0];
        let next_child_idx = bindings
            .iter()
            .position(|bind| bind == target_bind)
            .ok_or_else(|| ParseError::Custom("Key sequence not found".into()))?;

        if remaining.len() == 1 {
            let next_child = Arc::make_mut(&mut children[next_child_idx]);
            match next_child {
                KeyItem::Leaf(meta_idx, _) => {
                    *meta_idx = Some(metadata_index);
                }
                KeyItem::Tree(_, _, meta_idx) => {
                    *meta_idx = Some(metadata_index);
                }
            }
            Ok(())
        } else {
            Self::update_child_metadata(
                &mut children[next_child_idx],
                &remaining[1..],
                metadata_index,
            )
        }
    }

    pub fn register(
        &mut self,
        resolver: &Resolver,
        bind_sequence: Vec<UnresolvedKeyBind>,
        action: A,
        metadata: Option<M>,
    ) -> Result<(), ParseError> {
        if bind_sequence.is_empty() {
            return Err(ParseError::Custom("Empty keybind sequence".into()));
        }

        let first_resolved = resolver.resolve(bind_sequence[0].clone())?;

        let metadata_index = metadata.map(|x| {
            let res = self.metadata.len();
            self.metadata.push(x);
            res
        });

        for resolved_key in first_resolved {
            if bind_sequence.len() == 1 {
                // Add to the vector of items for this key
                self.tree
                    .entry(resolved_key)
                    .or_default()
                    .push(Arc::new(KeyItem::Leaf(metadata_index, action.clone())));
            } else {
                self.register_sequence(
                    resolved_key,
                    &bind_sequence[1..],
                    action.clone(),
                    metadata_index,
                )?;
            }
        }

        Ok(())
    }

    fn register_sequence(
        &mut self,
        first_key: ResolvedKeyBind,
        remaining: &[UnresolvedKeyBind],
        action: A,
        metadata_index: Option<usize>,
    ) -> Result<(), ParseError> {
        if remaining.is_empty() {
            return Err(ParseError::Custom("Empty remaining sequence".into()));
        }

        let items = self.tree.entry(first_key.clone()).or_default();

        // Try to find an existing tree node we can add to
        let mut added_to_existing = false;
        for item in items.iter_mut() {
            if let KeyItem::Tree(bindings, children, _) = Arc::make_mut(item) {
                // Check if this binding already exists in this tree
                if let Some(idx) = bindings.iter().position(|b| b == &remaining[0]) {
                    // Path already exists, recurse into it
                    if remaining.len() == 1 {
                        return Err(ParseError::Custom("Duplicate keybind".into()));
                    }
                    Self::add_to_child(
                        &mut children[idx],
                        &remaining[1..],
                        action.clone(),
                        metadata_index,
                    )?;
                    added_to_existing = true;
                    break;
                } else {
                    // Add new child to this existing tree
                    let new_child = Self::build_child(remaining, action.clone(), metadata_index)?;
                    bindings.push(remaining[0].clone());
                    children.push(Arc::new(new_child));
                    added_to_existing = true;
                    break;
                }
            }
        }

        if !added_to_existing {
            // No existing tree node, create a new one
            let new_child = Self::build_child(remaining, action, metadata_index)?;
            let tree_node =
                KeyItem::Tree(vec![remaining[0].clone()], vec![Arc::new(new_child)], None);
            items.push(Arc::new(tree_node));
        }

        Ok(())
    }

    fn add_to_child(
        child: &mut Arc<KeyItem<A>>,
        remaining: &[UnresolvedKeyBind],
        action: A,
        metadata_index: Option<usize>,
    ) -> Result<(), ParseError> {
        if remaining.is_empty() {
            return Err(ParseError::Custom("Empty remaining sequence".into()));
        }

        let child_mut = Arc::make_mut(child);

        match child_mut {
            KeyItem::Tree(bindings, children, _) => {
                // Check if this binding already exists
                if let Some(idx) = bindings.iter().position(|b| b == &remaining[0]) {
                    if remaining.len() == 1 {
                        return Err(ParseError::Custom("Duplicate keybind".into()));
                    }
                    Self::add_to_child(&mut children[idx], &remaining[1..], action, metadata_index)
                } else {
                    // Add new child to this tree
                    let new_child = Self::build_child(remaining, action, metadata_index)?;
                    bindings.push(remaining[0].clone());
                    children.push(Arc::new(new_child));
                    Ok(())
                }
            }
            KeyItem::Leaf(_, _) => Err(ParseError::Custom("Cannot add child to leaf node".into())),
        }
    }

    fn build_child(
        bind_sequence: &[UnresolvedKeyBind],
        action: A,
        metadata_index: Option<usize>,
    ) -> Result<KeyItem<A>, ParseError> {
        if bind_sequence.is_empty() {
            return Err(ParseError::Custom("Empty sequence".into()));
        }

        if bind_sequence.len() == 1 {
            Ok(KeyItem::Leaf(metadata_index, action))
        } else {
            let child = Self::build_child(&bind_sequence[1..], action, metadata_index)?;
            Ok(KeyItem::Tree(
                vec![bind_sequence[0].clone()],
                vec![Arc::new(child)],
                None,
            ))
        }
    }

    pub fn step(
        &mut self,
        resolver: &Resolver,
        key_code: KeyCode,
        key_mods: KeyModifiers,
        check: impl Fn(Option<&M>) -> bool,
    ) -> Result<StepResult<A>, ParseError> {
        let pressed_key = ResolvedKeyBind::new(key_mods, key_code);

        if self.active_tree.is_none() {
            if let Some(items) = self.tree.get(&pressed_key) {
                // Check items in reverse order (most recently added first)
                for (vec_idx, item) in items.iter().enumerate().rev() {
                    let meta = match item.as_ref() {
                        KeyItem::Leaf(meta_idx, _) | KeyItem::Tree(_, _, meta_idx) => {
                            meta_idx.and_then(|idx| self.metadata.get(idx))
                        }
                    };

                    if check(meta) {
                        match item.as_ref() {
                            KeyItem::Leaf(_, action) => {
                                self.current_sequence.clear();
                                return Ok(StepResult::Success(vec![pressed_key], action.clone()));
                            }
                            KeyItem::Tree(_, _, _) => {
                                self.active_tree = Some((0, Arc::clone(item), vec_idx));
                                self.current_sequence.push(pressed_key);
                                self.resolve_current_layer(resolver)?;
                                return Ok(StepResult::Step);
                            }
                        }
                    }
                }
            }
            return Ok(StepResult::Reset);
        }

        if let Some(cache) = &self.resolved_cache
            && let Some(candidates) = cache.get(&pressed_key)
        {
            // Check candidates in reverse order
            for &(vec_idx, child_idx) in candidates.iter().rev() {
                if let Some(active) = &self.active_tree
                    && let KeyItem::Tree(_, children, _) = active.1.as_ref()
                    && let Some(child) = children.get(child_idx)
                {
                    let meta = match child.as_ref() {
                        KeyItem::Leaf(meta_idx, _) | KeyItem::Tree(_, _, meta_idx) => {
                            meta_idx.and_then(|idx| self.metadata.get(idx))
                        }
                    };

                    if check(meta) {
                        self.current_sequence.push(pressed_key.clone());

                        match child.as_ref() {
                            KeyItem::Leaf(_, action) => {
                                let action = action.clone();
                                let seq = self.current_sequence.clone();
                                self.reset();
                                return Ok(StepResult::Success(seq, action));
                            }
                            KeyItem::Tree(_, _, _) => {
                                self.active_tree = Some((active.0 + 1, Arc::clone(child), vec_idx));
                                self.resolve_current_layer(resolver)?;
                                return Ok(StepResult::Step);
                            }
                        }
                    }
                }
            }
        }

        self.reset();
        Ok(StepResult::Reset)
    }

    fn resolve_current_layer(&mut self, resolver: &Resolver) -> Result<(), ParseError> {
        let mut cache: IndexMap<ResolvedKeyBind, Vec<(usize, usize)>> = IndexMap::new();

        if let Some((_, active, _)) = &self.active_tree
            && let KeyItem::Tree(unresolved_binds, _, _) = active.as_ref()
        {
            for (child_idx, unresolved_bind) in unresolved_binds.iter().enumerate() {
                let resolved_binds = resolver.resolve(unresolved_bind.clone())?;
                for resolved_bind in resolved_binds {
                    // For nested trees, we use vec_idx=0 since children don't have multiple versions
                    cache.entry(resolved_bind).or_default().push((0, child_idx));
                }
            }
        }

        self.resolved_cache = Some(cache);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.active_tree = None;
        self.resolved_cache = None;
        self.current_sequence.clear();
    }

    pub fn current_sequence(&self) -> &[ResolvedKeyBind] {
        &self.current_sequence
    }

    pub fn collect_layer_metadata(&self) -> Result<Vec<(ResolvedKeyBind, Option<M>)>, ParseError> {
        let mut result = vec![];

        if let (Some((_, active, _)), Some(cache)) = (&self.active_tree, &self.resolved_cache) {
            if let KeyItem::Tree(_, children, _) = active.as_ref() {
                for (resolved_key, candidates) in cache {
                    // Only show the first valid candidate (most recent)
                    if let Some(&(_, child_idx)) = candidates.first()
                        && let Some(child) = children.get(child_idx)
                    {
                        let meta = match child.as_ref() {
                            KeyItem::Leaf(meta_idx, _) => {
                                meta_idx.and_then(|i| self.metadata.get(i).cloned())
                            }
                            KeyItem::Tree(_, _, meta_idx) => {
                                meta_idx.and_then(|i| self.metadata.get(i).cloned())
                            }
                        };

                        result.push((resolved_key.clone(), meta));
                    }
                }
            }
        } else {
            for (resolved_key, items) in &self.tree {
                // Only show the most recent item
                if let Some(item) = items.last() {
                    let meta = match item.as_ref() {
                        KeyItem::Leaf(meta_idx, _) => {
                            meta_idx.and_then(|i| self.metadata.get(i).cloned())
                        }
                        KeyItem::Tree(_, _, meta_idx) => {
                            meta_idx.and_then(|i| self.metadata.get(i).cloned())
                        }
                    };

                    result.push((resolved_key.clone(), meta));
                }
            }
        }

        Ok(result)
    }

    pub fn active_tree(&self) -> Option<&KeyItem<A>> {
        self.active_tree.as_ref().map(|x| x.1.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepResult<A: Clone> {
    Success(Vec<ResolvedKeyBind>, A),
    Step,
    Reset,
}
