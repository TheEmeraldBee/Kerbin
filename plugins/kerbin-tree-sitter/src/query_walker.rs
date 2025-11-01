use std::collections::HashMap;
use std::sync::Arc;

use ropey::Rope;
use tree_sitter::{Query, QueryCursor, QueryMatch, StreamingIterator};

use crate::{state::TreeSitterState, text_provider::TextProviderRope};

/// Represents a match from either the main tree or an injected tree
#[derive(Debug)]
pub struct QueryMatchEntry<'a> {
    /// The match itself
    pub query_match: &'a QueryMatch<'a, 'a>,

    /// The query that produced this match
    pub query: Arc<Query>,

    /// The language this match came from
    pub lang: String,

    /// Whether this match came from an injected tree
    pub is_injected: bool,

    /// Index of the injected tree (None if from main tree)
    pub injected_index: Option<usize>,
}

/// Callback-based walker that processes all query matches in a tree and its injected trees
pub struct QueryWalker<'tree, 'rope> {
    /// Reference to the tree-sitter state
    state: &'tree TreeSitterState,

    /// The rope containing the text
    rope: &'rope Rope,

    /// The query for the main tree
    main_query: Arc<Query>,

    /// Queries for injected languages (keyed by language name)
    injected_queries: HashMap<String, Arc<Query>>,

    /// Current cursor
    cursor: QueryCursor,
}

impl<'tree, 'rope> QueryWalker<'tree, 'rope> {
    /// Creates a new QueryWalker with a single query for all trees
    pub fn new(state: &'tree TreeSitterState, rope: &'rope Rope, query: Arc<Query>) -> Self {
        Self {
            state,
            rope,
            main_query: query,
            injected_queries: HashMap::new(),
            cursor: QueryCursor::new(),
        }
    }

    /// Creates a QueryWalker with separate queries for injected languages
    pub fn new_with_injected_queries(
        state: &'tree TreeSitterState,
        rope: &'rope Rope,
        main_query: Arc<Query>,
        injected_queries: HashMap<String, Arc<Query>>,
    ) -> Self {
        Self {
            state,
            rope,
            main_query,
            injected_queries,
            cursor: QueryCursor::new(),
        }
    }

    /// Creates a QueryWalker with a custom QueryCursor configuration
    pub fn new_with_cursor(
        state: &'tree TreeSitterState,
        rope: &'rope Rope,
        query: Arc<Query>,
        mut cursor_config: impl FnMut(&mut QueryCursor),
    ) -> Self {
        let mut cursor = QueryCursor::new();
        cursor_config(&mut cursor);

        Self {
            state,
            rope,
            main_query: query,
            injected_queries: HashMap::new(),
            cursor,
        }
    }

    /// Creates a QueryWalker with injected queries and custom cursor configuration
    pub fn new_with_injected_queries_and_cursor(
        state: &'tree TreeSitterState,
        rope: &'rope Rope,
        main_query: Arc<Query>,
        injected_queries: HashMap<String, Arc<Query>>,
        mut cursor_config: impl FnMut(&mut QueryCursor),
    ) -> Self {
        let mut cursor = QueryCursor::new();
        cursor_config(&mut cursor);

        Self {
            state,
            rope,
            main_query,
            injected_queries,
            cursor,
        }
    }

    /// Walk through all matches, calling the callback for each one
    /// Returns early if the callback returns false
    pub fn walk<F>(&mut self, mut callback: F)
    where
        F: FnMut(QueryMatchEntry) -> bool,
    {
        let text_provider = TextProviderRope(self.rope);

        // Process main tree
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
                query_match: query_match,
                query: self.main_query.clone(),
                lang: self.state.lang.clone(),
                is_injected: false,
                injected_index: None,
            };

            if !callback(entry) {
                return;
            }
        }

        // Process injected trees
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
                    query_match: query_match,
                    query: query.clone(),
                    lang: injected.lang.clone(),
                    is_injected: true,
                    injected_index: Some(idx),
                };

                if !callback(entry) {
                    return;
                }
            }
        }
    }

    /// Walk through matches and collect them into a Vec
    /// This copies the match data so it can be stored
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
            });
            true
        });

        results
    }

    /// Walk through matches, but only process the first N matches
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

/// A stored version of a query match that doesn't hold references
#[derive(Debug, Clone)]
pub struct StoredQueryMatch {
    pub pattern_index: usize,
    pub captures: Vec<StoredCapture>,
    pub query: Arc<Query>,
    pub lang: String,
    pub is_injected: bool,
    pub injected_index: Option<usize>,
}

/// A stored version of a capture that doesn't hold references
#[derive(Debug, Clone)]
pub struct StoredCapture {
    pub node_id: usize,
    pub byte_range: std::ops::Range<usize>,
    pub capture_index: u32,
}

/// Builder pattern for creating QueryWalkers with custom configuration
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
    /// Creates a new builder with a single query
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

    /// Creates a new builder with separate queries for main and injected trees
    pub fn new_with_injected_queries(
        state: &'tree TreeSitterState,
        rope: &'rope Rope,
        main_query: Arc<Query>,
        injected_queries: HashMap<String, Arc<Query>>,
    ) -> Self {
        Self {
            state,
            rope,
            main_query,
            injected_queries,
            byte_range: None,
            point_range: None,
            match_limit: None,
        }
    }

    /// Adds a query for a specific injected language
    pub fn with_injected_query(mut self, lang: String, query: Arc<Query>) -> Self {
        self.injected_queries.insert(lang, query);
        self
    }

    /// Sets the byte range to search within
    pub fn byte_range(mut self, range: std::ops::Range<usize>) -> Self {
        self.byte_range = Some(range);
        self
    }

    /// Sets the point range to search within
    pub fn point_range(mut self, range: std::ops::Range<tree_sitter::Point>) -> Self {
        self.point_range = Some(range);
        self
    }

    /// Sets the maximum number of matches to return
    pub fn match_limit(mut self, limit: u32) -> Self {
        self.match_limit = Some(limit);
        self
    }

    /// Builds the QueryWalker with the configured settings
    pub fn build(self) -> QueryWalker<'tree, 'rope> {
        QueryWalker::new_with_injected_queries_and_cursor(
            self.state,
            self.rope,
            self.main_query,
            self.injected_queries,
            |cursor| {
                if let Some(range) = self.byte_range.clone() {
                    cursor.set_byte_range(range);
                }
                if let Some(range) = self.point_range.clone() {
                    cursor.set_point_range(range);
                }
                if let Some(limit) = self.match_limit {
                    cursor.set_match_limit(limit);
                }
            },
        )
    }
}
