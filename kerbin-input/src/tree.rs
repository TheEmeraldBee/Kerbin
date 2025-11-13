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
    /// The main key binding tree storage
    tree: IndexMap<ResolvedKeyBind, Arc<KeyItem<A>>>,

    /// Metadata for a item in the tree
    metadata: Vec<M>,

    /// Stack of active tree nodes as we descend
    active_tree: Option<(usize, Arc<KeyItem<A>>)>,

    /// Lazily resolved bindings for the current level only
    /// Maps resolved keys to indices in the active tree's children
    resolved_cache: Option<IndexMap<ResolvedKeyBind, usize>>,

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
                if let Some(item) = self.tree.get_mut(&resolved_key) {
                    match Arc::make_mut(item) {
                        KeyItem::Leaf(meta_idx, _) => {
                            *meta_idx = Some(metadata_index);
                        }
                        KeyItem::Tree(_, _, meta_idx) => {
                            *meta_idx = Some(metadata_index);
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

        let existing = self
            .tree
            .get_mut(&first_key)
            .ok_or_else(|| ParseError::Custom("Key not found in tree".into()))?;

        let KeyItem::Tree(bindings, children, _) = Arc::make_mut(existing) else {
            return Err(ParseError::Custom("Expected tree node, found leaf".into()));
        };

        let target_bind = &remaining[0];
        let child_idx = bindings
            .iter()
            .position(|bind| bind == target_bind)
            .ok_or_else(|| ParseError::Custom("Key sequence not found".into()))?;

        if remaining.len() == 1 {
            // We've reached the target node
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
                self.tree.insert(
                    resolved_key,
                    Arc::new(KeyItem::Leaf(metadata_index, action.clone())),
                );
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

        let tree_node = {
            if self.tree.contains_key(&first_key) {
                let existing = self
                    .tree
                    .get_mut(&first_key)
                    .expect("Just checked for existant");
                if let KeyItem::Tree(bindings, children, _) = Arc::make_mut(existing) {
                    let new_child = Self::build_child(remaining, action, metadata_index)?;
                    bindings.push(remaining[0].clone());
                    children.push(Arc::new(new_child));
                    return Ok(());
                } else {
                    return Err(ParseError::Custom("Key conflict: leaf exists".into()));
                }
            } else {
                let child = Self::build_child(remaining, action, metadata_index)?;
                KeyItem::Tree(vec![(remaining[0].clone())], vec![Arc::new(child)], None)
            }
        };

        self.tree.insert(first_key, Arc::new(tree_node));
        Ok(())
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
                vec![(bind_sequence[0].clone())],
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
    ) -> Result<StepResult<A>, ParseError> {
        let pressed_key = ResolvedKeyBind::new(key_mods, key_code);

        if self.active_tree.is_none() {
            if let Some(item) = self.tree.get(&pressed_key) {
                match item.as_ref() {
                    KeyItem::Leaf(_, action) => {
                        self.current_sequence.clear();
                        return Ok(StepResult::Success(
                            vec![ResolvedKeyBind {
                                mods: key_mods,
                                code: key_code,
                            }],
                            action.clone(),
                        ));
                    }
                    KeyItem::Tree(_, _, _) => {
                        self.active_tree = Some((0, Arc::clone(item)));
                        self.current_sequence.push(pressed_key);
                        // Lazily resolve only when needed
                        self.resolve_current_layer(resolver)?;
                        return Ok(StepResult::Step);
                    }
                }
            }
            return Ok(StepResult::Reset);
        }

        if let Some(cache) = &self.resolved_cache
            && let Some(&child_idx) = cache.get(&pressed_key)
        {
            self.current_sequence.push(pressed_key);

            // Get the child from the active tree
            if let Some(active) = &self.active_tree
                && let KeyItem::Tree(_, children, _) = active.1.as_ref()
            {
                let child = &children[child_idx];

                match child.as_ref() {
                    KeyItem::Leaf(_, action) => {
                        let action = action.clone();
                        let seq = self.current_sequence.clone();

                        self.reset();

                        return Ok(StepResult::Success(seq, action));
                    }
                    KeyItem::Tree(_, _, _) => {
                        self.active_tree = Some((active.0 + 1, Arc::clone(child)));
                        // Clear old cache and resolve new level
                        self.resolve_current_layer(resolver)?;
                        return Ok(StepResult::Step);
                    }
                }
            }
        }

        self.reset();
        Ok(StepResult::Reset)
    }

    fn resolve_current_layer(&mut self, resolver: &Resolver) -> Result<(), ParseError> {
        let mut cache = IndexMap::new();

        if let Some((_, active)) = &self.active_tree
            && let KeyItem::Tree(unresolved_binds, _, _) = active.as_ref()
        {
            for (idx, unresolved_bind) in unresolved_binds.iter().enumerate() {
                let resolved_binds = resolver.resolve(unresolved_bind.clone())?;
                for resolved_bind in resolved_binds {
                    cache.insert(resolved_bind, idx);
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

        if let (Some((_, active)), Some(cache)) = (&self.active_tree, &self.resolved_cache) {
            if let KeyItem::Tree(_, children, _) = active.as_ref() {
                for (resolved_key, &child_idx) in cache {
                    if let Some(child) = children.get(child_idx) {
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
            for (resolved_key, item) in &self.tree {
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
