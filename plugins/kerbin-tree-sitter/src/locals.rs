use std::ops::Range;
use std::sync::Arc;

use kerbin_core::*;
use ropey::Rope;
use tree_sitter::Query;

use crate::{
    grammar_manager::GrammarManager, query_walker::QueryWalkerBuilder, state::TreeSitterState,
};

pub struct Scope {
    pub byte_range: Range<usize>,
    pub definitions: Vec<Definition>,
    pub children: Vec<Scope>,
}

pub struct Definition {
    pub name: String,
    pub byte_range: Range<usize>,
}

pub struct Reference {
    pub name: String,
    pub byte_range: Range<usize>,
}

pub struct LocalsAnalysis {
    pub root_scope: Scope,
    pub references: Vec<Reference>,
}

struct RawScope {
    byte_range: Range<usize>,
}

struct RawDefinition {
    name: String,
    byte_range: Range<usize>,
}

pub fn build_locals_analysis(
    state: &TreeSitterState,
    rope: &Rope,
    query: Arc<Query>,
) -> LocalsAnalysis {
    let mut raw_scopes: Vec<RawScope> = Vec::new();
    let mut raw_defs: Vec<RawDefinition> = Vec::new();
    let mut raw_refs: Vec<Reference> = Vec::new();

    let mut walker = QueryWalkerBuilder::new(state, rope, query).build();

    walker.walk(|entry| {
        if entry.is_injected {
            return true;
        }

        let query = &entry.query;
        for capture in entry.query_match.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let start = capture.node.start_byte();
            let end = capture.node.end_byte();
            let byte_range = start..end;

            if capture_name.starts_with("local.scope") {
                raw_scopes.push(RawScope { byte_range });
            } else if capture_name.starts_with("local.definition") {
                let name = extract_text(rope, start, end);
                raw_defs.push(RawDefinition { name, byte_range });
            } else if capture_name == "local.reference" {
                let name = extract_text(rope, start, end);
                raw_refs.push(Reference { name, byte_range });
            }
        }
        true
    });

    let file_len = rope.len_bytes();
    let root_scope = build_scope_tree(raw_scopes, raw_defs, file_len);

    if raw_refs.is_empty()
        && let Some(tree) = &state.tree
    {
        let mut scope_defs: Vec<(String, Range<usize>)> = Vec::new();
        collect_scope_definitions(&root_scope, &mut scope_defs);
        if !scope_defs.is_empty() {
            synthesize_references(tree.root_node(), rope, &scope_defs, &mut raw_refs);
        }
    }

    LocalsAnalysis {
        root_scope,
        references: raw_refs,
    }
}

fn collect_scope_definitions(scope: &Scope, out: &mut Vec<(String, Range<usize>)>) {
    for def in &scope.definitions {
        out.push((def.name.clone(), scope.byte_range.clone()));
    }
    for child in &scope.children {
        collect_scope_definitions(child, out);
    }
}

fn synthesize_references(
    node: tree_sitter::Node,
    rope: &Rope,
    scope_defs: &[(String, Range<usize>)],
    refs: &mut Vec<Reference>,
) {
    if node.is_named() && node.child_count() == 0 {
        let start = node.start_byte();
        let end = node.end_byte();
        let text = extract_text(rope, start, end);
        for (name, scope_range) in scope_defs {
            if *name == text && scope_range.start <= start && start < scope_range.end {
                refs.push(Reference {
                    name: text,
                    byte_range: start..end,
                });
                break;
            }
        }
    } else {
        for i in 0..node.child_count() {
            synthesize_references(node.child(i as u32).unwrap(), rope, scope_defs, refs);
        }
    }
}

fn extract_text(rope: &Rope, start_byte: usize, end_byte: usize) -> String {
    let char_start = rope.byte_to_char(start_byte);
    let char_end = rope.byte_to_char(end_byte);
    rope.slice(char_start..char_end).to_string()
}

fn build_scope_tree(
    mut raw_scopes: Vec<RawScope>,
    raw_defs: Vec<RawDefinition>,
    file_len: usize,
) -> Scope {
    raw_scopes.sort_by_key(|s| s.byte_range.start);

    let mut scopes: Vec<Scope> = raw_scopes
        .into_iter()
        .map(|s| Scope {
            byte_range: s.byte_range,
            definitions: Vec::new(),
            children: Vec::new(),
        })
        .collect();

    for def in raw_defs {
        let mut best: Option<usize> = None;
        let mut best_size = usize::MAX;
        for (i, scope) in scopes.iter().enumerate() {
            if scope.byte_range.start <= def.byte_range.start
                && scope.byte_range.end >= def.byte_range.start
            {
                let size = scope.byte_range.end - scope.byte_range.start;
                if size < best_size {
                    best_size = size;
                    best = Some(i);
                }
            }
        }
        if let Some(idx) = best {
            scopes[idx].definitions.push(Definition {
                name: def.name,
                byte_range: def.byte_range,
            });
        }
    }

    let nested = nest_scopes(scopes);

    Scope {
        byte_range: 0..file_len,
        definitions: Vec::new(),
        children: nested,
    }
}

fn nest_scopes(scopes: Vec<Scope>) -> Vec<Scope> {
    let mut result: Vec<Scope> = Vec::new();
    let mut iter = scopes.into_iter().peekable();
    collect_children(&mut iter, usize::MAX, &mut result);
    result
}

fn collect_children(
    iter: &mut std::iter::Peekable<impl Iterator<Item = Scope>>,
    parent_end: usize,
    out: &mut Vec<Scope>,
) {
    while let Some(next) = iter.peek() {
        if next.byte_range.start >= parent_end {
            break;
        }
        let mut scope = iter.next().unwrap();
        let scope_end = scope.byte_range.end;
        collect_children(iter, scope_end, &mut scope.children);
        out.push(scope);
    }
}

pub fn find_highlight_ranges(cursor_byte: usize, analysis: &LocalsAnalysis) -> Vec<Range<usize>> {
    let ref_at_cursor = analysis
        .references
        .iter()
        .find(|r| r.byte_range.start <= cursor_byte && cursor_byte < r.byte_range.end);

    let definition = if let Some(r) = ref_at_cursor {
        resolve_definition(&analysis.root_scope, &r.name, r.byte_range.start)
    } else {
        find_definition_at_cursor(&analysis.root_scope, cursor_byte)
    };

    let Some((def, owning_scope_range)) = definition else {
        return Vec::new();
    };

    let mut ranges = Vec::new();
    ranges.push(def.byte_range.clone());

    for r in &analysis.references {
        if r.name == def.name
            && owning_scope_range.start <= r.byte_range.start
            && r.byte_range.start < owning_scope_range.end
            && !ranges.contains(&r.byte_range)
        {
            ranges.push(r.byte_range.clone());
        }
    }

    ranges
}

fn resolve_definition<'a>(
    scope: &'a Scope,
    name: &str,
    ref_start: usize,
) -> Option<(&'a Definition, Range<usize>)> {
    for child in &scope.children {
        if child.byte_range.start <= ref_start && ref_start < child.byte_range.end {
            if let Some(result) = resolve_definition(child, name, ref_start) {
                return Some(result);
            }
            for def in &child.definitions {
                if def.name == name {
                    return Some((def, child.byte_range.clone()));
                }
            }
            break;
        }
    }

    for def in &scope.definitions {
        if def.name == name {
            return Some((def, scope.byte_range.clone()));
        }
    }

    None
}

fn find_definition_at_cursor(
    scope: &Scope,
    cursor_byte: usize,
) -> Option<(&Definition, Range<usize>)> {
    for child in &scope.children {
        if child.byte_range.start <= cursor_byte
            && cursor_byte < child.byte_range.end
            && let Some(r) = find_definition_at_cursor(child, cursor_byte)
        {
            return Some(r);
        }
    }

    for def in &scope.definitions {
        if def.byte_range.start <= cursor_byte && cursor_byte < def.byte_range.end {
            return Some((def, scope.byte_range.clone()));
        }
    }

    None
}

pub async fn update_locals(
    buffers: ResMut<Buffers>,
    grammars: ResMut<GrammarManager>,
    config_path: Res<ConfigFolder>,
    theme: Res<Theme>,
) {
    get!(mut buffers, mut grammars, config_path, theme);

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else { return; };

    let Some(mut state) = buf.get_state_mut::<TreeSitterState>().await else {
        return;
    };

    if state.locals_analysis.is_none() {
        let lang = state.lang.clone();
        let Some(query) = grammars.get_query(&config_path.0, &lang, "locals") else {
            return;
        };

        let analysis = build_locals_analysis(&state, buf.get_rope(), query);
        state.locals_analysis = Some(analysis);
        state.locals_cursor_byte = None;
    }

    let cursor_byte = buf.primary_cursor().get_cursor_byte();

    if state.locals_cursor_byte == Some(cursor_byte) {
        return;
    }

    state.locals_cursor_byte = Some(cursor_byte);

    let analysis = state.locals_analysis.as_ref().unwrap();

    let all_ref_ranges: Vec<Range<usize>> = analysis
        .references
        .iter()
        .filter(|r| resolve_definition(&analysis.root_scope, &r.name, r.byte_range.start).is_some())
        .map(|r| r.byte_range.clone())
        .collect();

    let highlighted_ranges = find_highlight_ranges(cursor_byte, analysis);

    drop(state);

    let namespace = "tree-sitter::locals";
    buf.renderer.clear_extmark_ns(namespace);

    if !all_ref_ranges.is_empty() {
        let ref_style = theme.get("ts.local.reference").unwrap_or_default();

        for range in all_ref_ranges {
            buf.add_extmark(
                ExtmarkBuilder::new_range(namespace, range)
                    .with_priority(1000)
                    .with_kind(ExtmarkKind::Highlight { style: ref_style }),
            );
        }
    }

    if !highlighted_ranges.is_empty() {
        let hl_style = theme
            .get("ts.local.ref-highlight")
            .or_else(|| theme.get("ui.selection"))
            .unwrap_or_default();

        for range in highlighted_ranges {
            buf.add_extmark(
                ExtmarkBuilder::new_range(namespace, range)
                    .with_priority(1001)
                    .with_kind(ExtmarkKind::Highlight { style: hl_style }),
            );
        }
    }
}
