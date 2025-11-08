use std::{collections::HashMap, sync::Arc};

use ascii_forge::window::{KeyCode, KeyModifiers};

use crate::{ParseError, ResolvedKeyBind, Resolver, UnresolvedKeyBind};

#[derive(Debug, Clone)]
pub enum KeyItem<A: Clone> {
    /// Stores indices into a Vec of unresolved bindings
    Tree(Vec<UnresolvedKeyBind>, Vec<Arc<KeyItem<A>>>),
    Leaf(usize, A),
}

#[derive(Clone)]
pub struct KeyMetadata<M: Clone> {
    pub key_sequence: Vec<UnresolvedKeyBind>,
    pub metadata: M,
}

pub struct KeyTree<A: Clone, M: Clone> {
    /// The main key binding tree storage
    tree: HashMap<ResolvedKeyBind, Arc<KeyItem<A>>>,

    /// Metadata for a key
    metadata: Vec<KeyMetadata<M>>,

    /// Stack of active tree nodes as we descend
    active_tree: Option<Arc<KeyItem<A>>>,

    /// Lazily resolved bindings for the current level only
    /// Maps resolved keys to indices in the active tree's children
    resolved_cache: Option<HashMap<ResolvedKeyBind, usize>>,

    current_sequence: Vec<ResolvedKeyBind>,
}

impl<A: Clone, M: Clone> Default for KeyTree<A, M> {
    fn default() -> Self {
        Self {
            tree: HashMap::new(),
            metadata: vec![],

            active_tree: None,
            resolved_cache: None,

            current_sequence: vec![],
        }
    }
}

impl<A: Clone, M: Clone> KeyTree<A, M> {
    pub fn register(
        &mut self,
        resolver: &Resolver,
        bind_sequence: Vec<UnresolvedKeyBind>,
        action: A,
        metadata: M,
    ) -> Result<(), ParseError> {
        if bind_sequence.is_empty() {
            return Err(ParseError::Custom("Empty keybind sequence".into()));
        }

        let first_resolved = resolver.resolve(bind_sequence[0].clone())?;

        let metadata_index = self.metadata.len();
        self.metadata.push(KeyMetadata {
            metadata,
            key_sequence: bind_sequence.clone(),
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
        metadata_index: usize,
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
                if let KeyItem::Tree(bindings, children) = Arc::make_mut(existing) {
                    let new_child = Self::build_child(remaining, action, metadata_index)?;
                    bindings.push(remaining[0].clone());
                    children.push(Arc::new(new_child));
                    return Ok(());
                } else {
                    return Err(ParseError::Custom("Key conflict: leaf exists".into()));
                }
            } else {
                let child = Self::build_child(remaining, action, metadata_index)?;
                KeyItem::Tree(vec![remaining[0].clone()], vec![Arc::new(child)])
            }
        };

        self.tree.insert(first_key, Arc::new(tree_node));
        Ok(())
    }

    fn build_child(
        bind_sequence: &[UnresolvedKeyBind],
        action: A,
        metadata_index: usize,
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
            ))
        }
    }

    pub fn step(
        &mut self,
        resolver: &Resolver,
        key_code: KeyCode,
        key_mods: KeyModifiers,
    ) -> Result<StepResult<A>, ParseError> {
        let pressed_key = ResolvedKeyBind {
            mods: key_mods,
            code: key_code,
        };

        if self.active_tree.is_none() {
            if let Some(item) = self.tree.get(&pressed_key) {
                match item.as_ref() {
                    KeyItem::Leaf(_, action) => {
                        self.current_sequence.clear();
                        return Ok(StepResult::Success(action.clone()));
                    }
                    KeyItem::Tree(_, _) => {
                        self.active_tree = Some(Arc::clone(item));
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
                && let KeyItem::Tree(_, children) = active.as_ref()
            {
                let child = &children[child_idx];

                match child.as_ref() {
                    KeyItem::Leaf(_, action) => {
                        let action = action.clone();
                        self.reset();
                        return Ok(StepResult::Success(action));
                    }
                    KeyItem::Tree(_, _) => {
                        self.active_tree = Some(Arc::clone(child));
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
        let mut cache = HashMap::new();

        if let Some(active) = &self.active_tree
            && let KeyItem::Tree(unresolved_binds, _) = active.as_ref()
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

    pub fn collect_layer_metadata(&self) -> Vec<(usize, KeyMetadata<M>)> {
        let mut result = Vec::new();

        let Some(active) = &self.active_tree else {
            for item in self.tree.values() {
                Self::collect_from_item(item, &self.metadata, &mut result);
            }
            return result;
        };

        Self::collect_from_item(active, &self.metadata, &mut result);
        result
    }

    fn collect_from_item(
        item: &Arc<KeyItem<A>>,
        metadata: &[KeyMetadata<M>],
        result: &mut Vec<(usize, KeyMetadata<M>)>,
    ) {
        match item.as_ref() {
            KeyItem::Leaf(index, _) => {
                if let Some(meta) = metadata.get(*index) {
                    result.push((*index, meta.clone()));
                }
            }
            KeyItem::Tree(_, children) => {
                for child in children {
                    Self::collect_from_item(child, metadata, result);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepResult<A: Clone> {
    Success(A),
    Step,
    Reset,
}
