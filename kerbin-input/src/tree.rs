use std::sync::Arc;

use ascii_forge::window::{KeyCode, KeyModifiers};
use indexmap::IndexMap;

use crate::{Matchable, ParseError, ResolvedKeyBind, Resolver, UnresolvedKeyBind};

#[derive(Debug, Clone)]
pub enum KeyItem<A: Clone> {
    /// Tree nodes now also have multiple actions (for partial matches)
    /// Actions are stored in reverse order - last added is first checked
    Tree(
        Vec<UnresolvedKeyBind>,
        Vec<Arc<KeyItem<A>>>,
        Vec<(Option<usize>, A)>,
    ),
    /// Leaf nodes have multiple actions - last added is first checked
    Leaf(Vec<(Option<usize>, A)>),
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
                            KeyItem::Leaf(actions) => {
                                // Update the most recent action's metadata
                                if let Some((meta_idx, _)) = actions.last_mut() {
                                    *meta_idx = Some(metadata_index);
                                }
                            }
                            KeyItem::Tree(_, _, actions) => {
                                // Update the most recent action's metadata
                                if let Some((meta_idx, _)) = actions.last_mut() {
                                    *meta_idx = Some(metadata_index);
                                }
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
                KeyItem::Leaf(actions) => {
                    if let Some((meta_idx, _)) = actions.last_mut() {
                        *meta_idx = Some(metadata_index);
                    }
                }
                KeyItem::Tree(_, _, actions) => {
                    if let Some((meta_idx, _)) = actions.last_mut() {
                        *meta_idx = Some(metadata_index);
                    }
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
                KeyItem::Leaf(actions) => {
                    if let Some((meta_idx, _)) = actions.last_mut() {
                        *meta_idx = Some(metadata_index);
                    }
                }
                KeyItem::Tree(_, _, actions) => {
                    if let Some((meta_idx, _)) = actions.last_mut() {
                        *meta_idx = Some(metadata_index);
                    }
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
                // Try to add to existing leaf or create new one
                let items = self.tree.entry(resolved_key).or_default();

                let mut added_to_existing = false;
                // Try to find an existing Leaf to add to
                for item in items.iter_mut() {
                    if let KeyItem::Leaf(actions) = Arc::make_mut(item) {
                        // Push to existing leaf (last added = first checked)
                        actions.push((metadata_index, action.clone()));
                        added_to_existing = true;
                        break;
                    }
                }

                if !added_to_existing {
                    // Create new Leaf
                    items.push(Arc::new(KeyItem::Leaf(vec![(
                        metadata_index,
                        action.clone(),
                    )])));
                }
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
            let tree_node = KeyItem::Tree(
                vec![remaining[0].clone()],
                vec![Arc::new(new_child)],
                vec![],
            );
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
                    Self::add_to_child(&mut children[idx], &remaining[1..], action, metadata_index)
                } else {
                    // Add new child to this tree
                    let new_child = Self::build_child(remaining, action, metadata_index)?;
                    bindings.push(remaining[0].clone());
                    children.push(Arc::new(new_child));
                    Ok(())
                }
            }
            KeyItem::Leaf(actions) => {
                if remaining.len() == 1 {
                    // Add action to existing leaf (last added = first checked)
                    actions.push((metadata_index, action));
                    Ok(())
                } else {
                    Err(ParseError::Custom(
                        "Cannot add multi-key sequence to leaf node".into(),
                    ))
                }
            }
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
            Ok(KeyItem::Leaf(vec![(metadata_index, action)]))
        } else {
            let child = Self::build_child(&bind_sequence[1..], action, metadata_index)?;
            Ok(KeyItem::Tree(
                vec![bind_sequence[0].clone()],
                vec![Arc::new(child)],
                vec![],
            ))
        }
    }

    pub fn step(
        &mut self,
        resolver: &Resolver,
        key_code: KeyCode,
        key_mods: KeyModifiers,
        check: impl Fn(Option<&M>) -> Option<u32>,
    ) -> Result<StepResult<A>, ParseError> {
        let pressed_key = ResolvedKeyBind::new(key_mods, key_code);

        let candidates = [
            pressed_key.clone(),
            ResolvedKeyBind {
                mods: Matchable::Any,
                code: Matchable::Specific(pressed_key.code.unwrap_specific()),
            },
            ResolvedKeyBind {
                mods: Matchable::Specific(pressed_key.mods.unwrap_specific()),
                code: Matchable::Any,
            },
            ResolvedKeyBind {
                mods: Matchable::Any,
                code: Matchable::Any,
            },
        ];

        struct Match<A: Clone> {
            rank: u32,
            candidate_idx: usize,
            vec_idx: usize,
            action_idx: usize,
            result: PendingResult<A>,
        }

        enum PendingResult<A: Clone> {
            Success(A),
            Step(usize, Arc<KeyItem<A>>, usize),
        }

        let mut best_match: Option<Match<A>> = None;

        let mut consider = |rank: u32,
                            candidate_idx: usize,
                            vec_idx: usize,
                            action_idx: usize,
                            result: PendingResult<A>| {
            match &best_match {
                None => {
                    best_match = Some(Match {
                        rank,
                        candidate_idx,
                        vec_idx,
                        action_idx,
                        result,
                    })
                }
                Some(current) => {
                    if rank < current.rank {
                        best_match = Some(Match {
                            rank,
                            candidate_idx,
                            vec_idx,
                            action_idx,
                            result,
                        });
                    } else if rank == current.rank {
                        if candidate_idx < current.candidate_idx {
                            best_match = Some(Match {
                                rank,
                                candidate_idx,
                                vec_idx,
                                action_idx,
                                result,
                            });
                        } else if candidate_idx == current.candidate_idx {
                            if vec_idx > current.vec_idx {
                                best_match = Some(Match {
                                    rank,
                                    candidate_idx,
                                    vec_idx,
                                    action_idx,
                                    result,
                                });
                            } else if vec_idx == current.vec_idx && action_idx > current.action_idx
                            {
                                best_match = Some(Match {
                                    rank,
                                    candidate_idx,
                                    vec_idx,
                                    action_idx,
                                    result,
                                });
                            }
                        }
                    }
                }
            }
        };

        if self.active_tree.is_none() {
            for (cand_idx, candidate) in candidates.iter().enumerate() {
                if let Some(items) = self.tree.get(candidate) {
                    for (vec_idx, item) in items.iter().enumerate() {
                        match item.as_ref() {
                            KeyItem::Leaf(actions) => {
                                for (action_idx, (meta_idx, action)) in actions.iter().enumerate() {
                                    let meta = meta_idx.and_then(|idx| self.metadata.get(idx));
                                    if let Some(rank) = check(meta) {
                                        consider(
                                            rank,
                                            cand_idx,
                                            vec_idx,
                                            action_idx,
                                            PendingResult::Success(action.clone()),
                                        );
                                    }
                                }
                            }
                            KeyItem::Tree(_, _, actions) => {
                                // Check for actions (potential success/rank source)
                                let mut best_action_rank = None;
                                for (meta_idx, _) in actions.iter() {
                                    let meta = meta_idx.and_then(|idx| self.metadata.get(idx));
                                    if let Some(rank) = check(meta)
                                        && best_action_rank.is_none_or(|r| rank < r)
                                    {
                                        best_action_rank = Some(rank);
                                    }
                                }

                                if best_action_rank.is_some() || actions.is_empty() {
                                    let step_rank = best_action_rank.unwrap_or(u32::MAX);
                                    consider(
                                        step_rank,
                                        cand_idx,
                                        vec_idx,
                                        usize::MAX,
                                        PendingResult::Step(0, Arc::clone(item), vec_idx),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        } else if let Some(cache) = &self.resolved_cache {
            for (cand_idx, candidate) in candidates.iter().enumerate() {
                if let Some(matches) = cache.get(candidate) {
                    for &(vec_idx, child_idx) in matches.iter() {
                        if let Some(active) = &self.active_tree
                            && let KeyItem::Tree(_, children, _) = active.1.as_ref()
                            && let Some(child) = children.get(child_idx)
                        {
                            match child.as_ref() {
                                KeyItem::Leaf(actions) => {
                                    for (action_idx, (meta_idx, action)) in
                                        actions.iter().enumerate()
                                    {
                                        let meta = meta_idx.and_then(|idx| self.metadata.get(idx));
                                        if let Some(rank) = check(meta) {
                                            consider(
                                                rank,
                                                cand_idx,
                                                vec_idx, // This is 0 for nested
                                                action_idx,
                                                PendingResult::Success(action.clone()),
                                            );
                                        }
                                    }
                                }
                                KeyItem::Tree(_, _, actions) => {
                                    let mut best_action_rank = None;
                                    for (meta_idx, _) in actions.iter() {
                                        let meta = meta_idx.and_then(|idx| self.metadata.get(idx));
                                        if let Some(rank) = check(meta)
                                            && best_action_rank.is_none_or(|r| rank < r)
                                        {
                                            best_action_rank = Some(rank);
                                        }
                                    }

                                    if best_action_rank.is_some() || actions.is_empty() {
                                        let step_rank = best_action_rank.unwrap_or(u32::MAX);
                                        consider(
                                            step_rank,
                                            cand_idx,
                                            vec_idx,
                                            usize::MAX,
                                            PendingResult::Step(
                                                active.0 + 1,
                                                Arc::clone(child),
                                                vec_idx,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        match best_match {
            Some(Match { result, .. }) => match result {
                PendingResult::Success(action) => {
                    self.current_sequence.push(pressed_key);
                    let seq = self.current_sequence.clone();
                    self.reset();
                    Ok(StepResult::Success(seq, action))
                }
                PendingResult::Step(depth, node, idx) => {
                    self.current_sequence.push(pressed_key);
                    self.active_tree = Some((depth, node, idx));
                    self.resolve_current_layer(resolver)?;
                    Ok(StepResult::Step)
                }
            },
            None => {
                self.reset();
                Ok(StepResult::Reset)
            }
        }
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
                            KeyItem::Leaf(actions) => {
                                // Get metadata from most recent action
                                actions.last().and_then(|(meta_idx, _)| {
                                    meta_idx.and_then(|i| self.metadata.get(i).cloned())
                                })
                            }
                            KeyItem::Tree(_, _, actions) => {
                                // Get metadata from most recent action
                                actions.last().and_then(|(meta_idx, _)| {
                                    meta_idx.and_then(|i| self.metadata.get(i).cloned())
                                })
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
                        KeyItem::Leaf(actions) => {
                            // Get metadata from most recent action
                            actions.last().and_then(|(meta_idx, _)| {
                                meta_idx.and_then(|i| self.metadata.get(i).cloned())
                            })
                        }
                        KeyItem::Tree(_, _, actions) => {
                            // Get metadata from most recent action
                            actions.last().and_then(|(meta_idx, _)| {
                                meta_idx.and_then(|i| self.metadata.get(i).cloned())
                            })
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

impl<T: Copy> Matchable<T> {
    pub fn unwrap_specific(self) -> T {
        match self {
            Matchable::Specific(t) => t,
            Matchable::Any => panic!("Called unwrap_specific on Any"),
        }
    }
}
