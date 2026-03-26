use std::collections::HashMap;
use std::sync::Arc;

use ropey::Rope;
use tree_sitter::{Query, QueryCursor, QueryMatch, StreamingIterator};

use crate::{state::TreeSitterState, text_provider::TextProviderRope};

/// Represents a match from either the main tree or an injected tree
#[derive(Debug)]
pub struct QueryMatchEntry<'a> {
    pub query_match: &'a QueryMatch<'a, 'a>,
    pub query: Arc<Query>,
    pub lang: String,
    pub is_injected: bool,
    pub injected_index: Option<usize>,
    /// Byte offset to add to node positions (injected trees are positioned within the parent)
    pub byte_offset: usize,
}

/// Callback-based walker processing all query matches in a tree and its injected trees
pub struct QueryWalker<'tree, 'rope> {
    state: &'tree TreeSitterState,
    rope: &'rope Rope,
    main_query: Arc<Query>,
    injected_queries: HashMap<String, Arc<Query>>,
    cursor: QueryCursor,
}

impl<'tree, 'rope> QueryWalker<'tree, 'rope> {
    /// Walk all matches across the main tree and injected trees, stopping if callback returns false
    pub fn walk<F>(&mut self, mut callback: F)
    where
        F: FnMut(QueryMatchEntry) -> bool,
    {
        let text_provider = TextProviderRope(self.rope);

        let mut matches = self.cursor.matches(
            &self.main_query,
            self.state
                .tree
                .as_ref()
                .expect("Tree should only be none during reparse")
                .root_node(),
            &text_provider,
        );

        while let Some(query_match) = matches.next() {
            let entry = QueryMatchEntry {
                query_match,
                query: self.main_query.clone(),
                lang: self.state.lang.clone(),
                is_injected: false,
                injected_index: None,
                byte_offset: 0,
            };

            if !callback(entry) {
                return;
            }
        }

        for (idx, injected) in self.state.injected_trees.iter().enumerate() {
            // Use language-specific query if available, otherwise use main query
            let query = self
                .injected_queries
                .get(&injected.lang)
                .unwrap_or(&self.main_query);

            let mut matches = self
                .cursor
                .matches(query, injected.tree.root_node(), &text_provider);

            while let Some(query_match) = matches.next() {
                let entry = QueryMatchEntry {
                    query_match,
                    query: query.clone(),
                    lang: injected.lang.clone(),
                    is_injected: true,
                    injected_index: Some(idx),
                    byte_offset: injected.byte_range.start,
                };

                if !callback(entry) {
                    return;
                }
            }
        }
    }

    pub fn collect_matches(&mut self) -> Vec<StoredQueryMatch> {
        let mut results = Vec::new();

        self.walk(|entry| {
            results.push(StoredQueryMatch {
                pattern_index: entry.query_match.pattern_index,
                captures: entry
                    .query_match
                    .captures
                    .iter()
                    .map(|c| StoredCapture {
                        node_id: c.node.id(),
                        byte_range: c.node.byte_range(),
                        capture_index: c.index,
                    })
                    .collect(),
                query: entry.query.clone(),
                lang: entry.lang,
                is_injected: entry.is_injected,
                injected_index: entry.injected_index,
                byte_offset: entry.byte_offset,
            });
            true
        });

        results
    }

    pub fn walk_limited<F>(&mut self, limit: usize, mut callback: F)
    where
        F: FnMut(QueryMatchEntry),
    {
        let mut count = 0;
        self.walk(|entry| {
            if count >= limit {
                return false;
            }
            callback(entry);
            count += 1;
            true
        });
    }
}

/// Stored version of a query match that doesn't hold references
#[derive(Debug, Clone)]
pub struct StoredQueryMatch {
    pub pattern_index: usize,
    pub captures: Vec<StoredCapture>,
    pub query: Arc<Query>,
    pub lang: String,
    pub is_injected: bool,
    pub injected_index: Option<usize>,
    pub byte_offset: usize,
}

/// Stored version of a capture that doesn't hold references
#[derive(Debug, Clone)]
pub struct StoredCapture {
    pub node_id: usize,
    pub byte_range: std::ops::Range<usize>,
    pub capture_index: u32,
}

pub struct QueryWalkerBuilder<'tree, 'rope> {
    state: &'tree TreeSitterState,
    rope: &'rope Rope,
    main_query: Arc<Query>,
    injected_queries: HashMap<String, Arc<Query>>,
    byte_range: Option<std::ops::Range<usize>>,
    point_range: Option<std::ops::Range<tree_sitter::Point>>,
    match_limit: Option<u32>,
}

impl<'tree, 'rope> QueryWalkerBuilder<'tree, 'rope> {
    pub fn new(state: &'tree TreeSitterState, rope: &'rope Rope, query: Arc<Query>) -> Self {
        Self {
            state,
            rope,
            main_query: query,
            injected_queries: HashMap::new(),
            byte_range: None,
            point_range: None,
            match_limit: None,
        }
    }

    pub fn with_injected_query(mut self, lang: String, query: Arc<Query>) -> Self {
        self.injected_queries.insert(lang, query);
        self
    }

    pub fn with_injected_queries(mut self, injected_queries: HashMap<String, Arc<Query>>) -> Self {
        self.injected_queries.extend(injected_queries);
        self
    }

    pub fn byte_range(mut self, range: std::ops::Range<usize>) -> Self {
        self.byte_range = Some(range);
        self
    }

    pub fn point_range(mut self, range: std::ops::Range<tree_sitter::Point>) -> Self {
        self.point_range = Some(range);
        self
    }

    pub fn match_limit(mut self, limit: u32) -> Self {
        self.match_limit = Some(limit);
        self
    }

    pub fn build(self) -> QueryWalker<'tree, 'rope> {
        let mut cursor = QueryCursor::new();

        if let Some(range) = self.byte_range.clone() {
            cursor.set_byte_range(range);
        }
        if let Some(range) = self.point_range.clone() {
            cursor.set_point_range(range);
        }
        if let Some(limit) = self.match_limit {
            cursor.set_match_limit(limit);
        }

        QueryWalker {
            state: self.state,
            rope: self.rope,
            main_query: self.main_query,
            injected_queries: self.injected_queries,
            cursor,
        }
    }
}
