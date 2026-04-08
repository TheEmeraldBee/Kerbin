use kerbin_core::SafeRopeAccess;
use kerbin_core::*;
use ratatui::style::Style;
use tree_sitter::{Parser, Tree};

use crate::{
    grammar_manager::GrammarManager,
    highlighter::{HighlightSpan, Highlighter, merge_overlapping_spans},
    locals::LocalsAnalysis,
    query_walker::QueryWalkerBuilder,
};

fn translate_name_to_style(theme: &Theme, mut name: &str) -> Style {
    loop {
        if let Some(value) = theme.get(&format!("ts.{name}")) {
            return value;
        }

        if let Some(last_dot_index) = name.rfind('.') {
            name = &name[..last_dot_index];
        } else {
            break;
        }
    }

    theme.get("ui.text").unwrap_or_default()
}

pub struct InjectedTree {
    pub lang: String,
    pub tree: Tree,
    pub byte_range: std::ops::Range<usize>,
}

#[derive(State)]
pub struct TreeSitterState {
    pub lang: String,
    pub parser: Parser,
    pub tree: Option<Tree>,
    pub injected_trees: Vec<InjectedTree>,
    pub locals_analysis: Option<LocalsAnalysis>,
    pub locals_cursor_byte: Option<usize>,
}

pub async fn update_trees(
    buffers: ResMut<Buffers>,
    grammars: ResMut<GrammarManager>,
    config_path: Res<ConfigFolder>,
    log: Res<LogSender>,
) {
    get!(mut buffers, mut grammars, config_path, log);

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else { return; };

    if !buf.has_state::<TreeSitterState>() || buf.byte_changes.is_empty() {
        return;
    }

    let mut state = buf.get_state_mut::<TreeSitterState>().await.unwrap();

    for change in &buf.byte_changes {
        let [start, old_end, new_end] = change;

        let input_edit = tree_sitter::InputEdit {
            start_byte: start.1,
            old_end_byte: old_end.1,
            new_end_byte: new_end.1,
            start_position: tree_sitter::Point {
                row: start.0.0,
                column: start.0.1,
            },
            old_end_position: tree_sitter::Point {
                row: old_end.0.0,
                column: old_end.0.1,
            },
            new_end_position: tree_sitter::Point {
                row: new_end.0.0,
                column: new_end.0.1,
            },
        };

        state
            .tree
            .as_mut()
            .expect("Should only be none during reparse")
            .edit(&input_edit);

        for injected in &mut state.injected_trees {
            injected.tree.edit(&input_edit);
        }
    }

    let tree = state.tree.take();

    let new_tree = state.parser.parse_with_options(
        &mut |byte, _| {
            let (chunk, start_byte, _, _) = buf.chunk_at(byte).unwrap_or(("", 0, 0, 0));
            if chunk.is_empty() {
                return &[] as &[u8];
            }
            &chunk.as_bytes()[(byte - start_byte)..]
        },
        tree.as_ref(),
        None,
    );

    if let Some(new_tree) = new_tree {
        state.tree = Some(new_tree);
    } else {
        log.critical("tree-sitter::update_trees", "Failed to reparse main tree");
        state.tree = tree;
        return;
    }

    // Temporarily take ownership to avoid borrow conflicts when reloading injected trees
    let lang = state.lang.clone();
    let temp_state = TreeSitterState {
        lang: lang.clone(),
        parser: Parser::new(),
        tree: state.tree.clone(),
        injected_trees: vec![],
        locals_analysis: None,
        locals_cursor_byte: None,
    };
    state.locals_analysis = None;

    let injected_trees =
        load_injected_trees(&temp_state, &mut grammars, &config_path.0, buf.get_rope());

    state.injected_trees = injected_trees;
}

pub async fn open_files(
    buffers: ResMut<Buffers>,
    grammars: ResMut<GrammarManager>,
    config_path: Res<ConfigFolder>,
    theme: Res<Theme>,

    log: Res<LogSender>,
) {
    get!(mut buffers, mut grammars, config_path, theme, log);

    let Some(mut buf) = buffers.cur_text_buffer_mut().await else { return; };

    if buf.flags.contains("tree-sitter-checked") || buf.has_state::<TreeSitterState>() {
        return;
    }

    buf.flags.insert("tree-sitter-checked");

    let Some(lang) = grammars.ext_to_lang(&buf.ext).map(|x| x.to_string()) else {
        return;
    };

    let grammar = match grammars.get_grammar(&config_path.0, &lang) {
        Ok(g) => g,
        Err(e) => {
            log.critical(
                "tree-sitter::open_file",
                format!("Failed to load grammar due to error: {e}"),
            );
            return;
        }
    };

    let mut parser = Parser::new();
    match parser.set_language(&grammar.lang) {
        Ok(_) => {}
        Err(e) => {
            log.critical(
                "tree-sitter::open_file",
                format!("Failed to initialize parser due to error: {e}"),
            );
            return;
        }
    };

    let tree = parser.parse_with_options(
        &mut |byte, _| {
            let (chunk, start_byte, _, _) = buf.chunk_at(byte).unwrap_or(("", 0, 0, 0));
            if chunk.is_empty() {
                return &[] as &[u8];
            }
            &chunk.as_bytes()[(byte - start_byte)..]
        },
        None,
        None,
    );

    let Some(tree) = tree else {
        log.critical(
            "tree-sitter::open_file",
            "Tree Sitter failed to parse into tree",
        );
        return;
    };

    // Create initial state without injections
    let initial_state = TreeSitterState {
        lang: lang.clone(),
        parser,
        tree: Some(tree),
        injected_trees: vec![],
        locals_analysis: None,
        locals_cursor_byte: None,
    };

    // Load injected trees using the initial state
    let injected_trees = load_injected_trees(
        &initial_state,
        &mut grammars,
        &config_path.0,
        buf.get_rope(),
    );

    // Update state with injected trees
    buf.set_state(TreeSitterState {
        lang: initial_state.lang,
        parser: initial_state.parser,
        tree: initial_state.tree,
        injected_trees,
        locals_analysis: None,
        locals_cursor_byte: None,
    });

    let state = buf
        .get_state_mut::<TreeSitterState>()
        .await
        .expect("State just inserted");

    let Some(highlighter) = Highlighter::new(&config_path.0, &mut grammars, &state, buf.get_rope())
    else {
        return;
    };

    let spans = highlighter.collect_spans();

    buf.renderer.clear_extmark_ns("tree-sitter::highlights");

    emit_spans(spans, "tree-sitter::highlights", &mut buf, &theme);
}

pub fn emit_spans(spans: Vec<HighlightSpan>, namespace: &str, buf: &mut TextBuffer, theme: &Theme) {
    let (conceal_spans, highlight_spans): (Vec<_>, Vec<_>) = spans
        .into_iter()
        .partition(|s| s.is_conceal);

    for span in merge_overlapping_spans(highlight_spans) {
        buf.add_extmark(
            ExtmarkBuilder::new_range(namespace, span.byte_range.clone())
                .with_kind(ExtmarkKind::Highlight {
                    style: translate_name_to_style(theme, &span.capture_name),
                }),
        );
    }

    for span in conceal_spans {
        buf.add_extmark(
            ExtmarkBuilder::new_range(namespace, span.byte_range.clone())
                .with_kind(ExtmarkKind::Conceal {
                    replacement: None,
                    style: None,
                    scope: span.conceal_scope,
                    trim_before: span.trim_before,
                    trim_after: span.trim_after,
                }),
        );
    }
}

fn load_injected_trees(
    state: &TreeSitterState,
    grammars: &mut GrammarManager,
    config_path: &str,
    rope: &ropey::Rope,
) -> Vec<InjectedTree> {
    let mut injected_trees = Vec::new();

    let Some(injections_query) = grammars.get_query(config_path, &state.lang, "injections") else {
        return injected_trees;
    };

    let lang_capture_idx = injections_query
        .capture_names()
        .iter()
        .position(|name| *name == "injection.language");

    let content_capture_idx = injections_query
        .capture_names()
        .iter()
        .position(|name| *name == "injection.content");

    // Use QueryWalker to find all injection matches
    let mut walker = QueryWalkerBuilder::new(state, rope, injections_query.clone()).build();

    walker.walk(|entry| {
        let mut injection_lang: Option<String> = None;
        let mut content_nodes = Vec::new();

        for capture in entry.query_match.captures {
            if Some(capture.index as usize) == lang_capture_idx {
                let start_byte = capture.node.start_byte();
                let end_byte = capture.node.end_byte();
                let text = rope.slice(start_byte..end_byte).to_string();
                injection_lang = Some(text.trim_matches('"').to_string());
            } else if Some(capture.index as usize) == content_capture_idx {
                content_nodes.push(capture.node);
            }
        }

        if injection_lang.is_none() {
            for prop in injections_query.property_settings(entry.query_match.pattern_index) {
                if prop.key.as_ref() == "injection.language" {
                    injection_lang = Some(
                        prop.value
                            .as_ref()
                            .map(|x| x.to_string())
                            .unwrap_or_default(),
                    );
                }
            }
        }

        if let Some(inj_lang) = injection_lang {
            for content_node in content_nodes {
                match parse_injection(grammars, config_path, &inj_lang, content_node, rope) {
                    Ok(injected_tree) => {
                        injected_trees.push(injected_tree);
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse {} injection: {}", inj_lang, e);
                    }
                }
            }
        }

        true
    });

    injected_trees
}

fn parse_injection(
    grammars: &mut GrammarManager,
    config_path: &str,
    lang: &str,
    content_node: tree_sitter::Node,
    rope: &ropey::Rope,
) -> Result<InjectedTree, String> {
    let grammar = grammars
        .get_grammar(config_path, lang)
        .map_err(|e| format!("Failed to load grammar: {}", e))?;

    let mut parser = Parser::new();
    parser
        .set_language(&grammar.lang)
        .map_err(|e| format!("Failed to set language: {}", e))?;

    let start_byte = content_node.start_byte();
    let end_byte = content_node.end_byte();

    let tree = parser
        .parse_with_options(
            &mut |byte, _| {
                let adjusted_byte = start_byte + byte;
                if adjusted_byte >= end_byte {
                    return &[] as &[u8];
                }
                let (chunk, chunk_start, _, _) = rope.chunk_at_byte(adjusted_byte);
                let offset = adjusted_byte - chunk_start;
                let chunk_bytes = chunk.as_bytes();
                let available = (end_byte - adjusted_byte).min(chunk_bytes.len() - offset);
                &chunk_bytes[offset..offset + available]
            },
            None,
            None,
        )
        .ok_or_else(|| "Failed to parse injected tree".to_string())?;

    Ok(InjectedTree {
        lang: lang.to_string(),
        tree,
        byte_range: start_byte..end_byte,
    })
}

/// Highlights a given text string using the specified language
pub fn highlight_text(
    text: &str,
    lang: &str,
    grammars: &mut GrammarManager,
    config_path: &str,
    theme: &Theme,
    log: &LogSender,
) -> Vec<(String, Style)> {
    let rope = ropey::Rope::from_str(text);

    // Load the grammar
    let grammar = match grammars.get_grammar(config_path, lang) {
        Ok(g) => g,
        Err(e) => {
            log.low(
                "tree-sitter::highlight_text",
                format!("Failed to load grammar for {lang}: {e}"),
            );
            return vec![(text.to_string(), theme.get("ui.text").unwrap_or_default())];
        }
    };

    let mut parser = Parser::new();
    if let Err(e) = parser.set_language(&grammar.lang) {
        log.low(
            "tree-sitter::highlight_text",
            format!("Failed to set language for {lang}: {e}"),
        );
        return vec![(text.to_string(), theme.get("ui.text").unwrap_or_default())];
    }

    // Parse the text
    let tree = parser.parse_with_options(
        &mut |byte, _| {
            if byte >= text.len() {
                return &[] as &[u8];
            }
            &text.as_bytes()[byte..]
        },
        None,
        None,
    );

    let Some(tree) = tree else {
        return vec![(text.to_string(), theme.get("ui.text").unwrap_or_default())];
    };

    // Create state
    let state = TreeSitterState {
        lang: lang.to_string(),
        parser,
        tree: Some(tree),
        injected_trees: vec![],
        locals_analysis: None,
        locals_cursor_byte: None,
    };

    // Load injected trees (using the existing private function)
    let injected_trees = load_injected_trees(&state, grammars, config_path, &rope);

    // Update state with injections
    let state = TreeSitterState {
        injected_trees,
        ..state
    };

    // Create highlighter
    let Some(highlighter) = Highlighter::new(config_path, grammars, &state, &rope) else {
        return vec![(text.to_string(), theme.get("ui.text").unwrap_or_default())];
    };

    let spans = highlighter.collect_spans();
    let merged = merge_overlapping_spans(spans);

    let mut result = Vec::new();
    let mut last_idx = 0;

    for span in merged {
        // Skip spans entirely consumed by a previous trim_after
        if span.byte_range.end <= last_idx {
            continue;
        }

        if span.byte_range.start > last_idx {
            let raw = &text[last_idx..span.byte_range.start];
            let seg = if span.is_conceal && span.trim_before {
                raw.trim_end_matches(|c: char| c.is_ascii_whitespace())
            } else {
                raw
            };
            if !seg.is_empty() {
                result.push((seg.to_string(), theme.get("ui.text").unwrap_or_default()));
            }
        }

        if !span.is_conceal {
            let start = span.byte_range.start.max(last_idx);
            if start < span.byte_range.end {
                result.push((
                    text[start..span.byte_range.end].to_string(),
                    translate_name_to_style(theme, &span.capture_name),
                ));
            }
        }

        last_idx = span.byte_range.end;

        if span.is_conceal && span.trim_after {
            while last_idx < text.len() && text.as_bytes()[last_idx].is_ascii_whitespace() {
                last_idx += 1;
            }
        }
    }

    if last_idx < text.len() {
        result.push((
            text[last_idx..].to_string(),
            theme.get("ui.text").unwrap_or_default(),
        ));
    }

    result
}
