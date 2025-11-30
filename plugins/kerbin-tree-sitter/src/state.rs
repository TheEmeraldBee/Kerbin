use kerbin_core::{ascii_forge::window::ContentStyle, *};
use tree_sitter::{Parser, Tree};

use crate::{
    grammar_manager::GrammarManager,
    highlighter::{Highlighter, merge_overlapping_spans},
    query_walker::QueryWalkerBuilder,
};

fn translate_name_to_style(theme: &Theme, mut name: &str) -> ContentStyle {
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

/// A tree that was injected into the state
pub struct InjectedTree {
    pub lang: String,
    pub tree: Tree,
    pub byte_range: std::ops::Range<usize>,
}

/// A state stored in each buffer with the state of tree-sitter
#[derive(State)]
pub struct TreeSitterState {
    pub lang: String,
    pub parser: Parser,
    pub tree: Option<Tree>,
    pub injected_trees: Vec<InjectedTree>,
}

/// Updates tree-sitter trees based on byte changes in the buffer
pub async fn update_trees(
    buffers: ResMut<Buffers>,
    grammars: ResMut<GrammarManager>,
    config_path: Res<ConfigFolder>,
    log: Res<LogSender>,
) {
    get!(mut buffers, mut grammars, config_path, log);

    let mut buf = buffers.cur_buffer_mut().await;

    // Only process if buffer has tree-sitter state and byte changes
    if !buf.has_state::<TreeSitterState>() || buf.byte_changes.is_empty() {
        return;
    }

    // Get mutable state reference
    let mut state = buf.get_state_mut::<TreeSitterState>().await.unwrap();

    // Convert byte changes to tree-sitter InputEdit format
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

        // Apply edit to main tree
        state
            .tree
            .as_mut()
            .expect("Should only be none during reparse")
            .edit(&input_edit);

        // Apply edit to all injected trees
        for injected in &mut state.injected_trees {
            injected.tree.edit(&input_edit);
        }
    }

    let tree = state.tree.take();

    // Reparse main tree
    let new_tree = state.parser.parse_with_options(
        &mut |byte, _| {
            let (chunk, start_byte) = buf.rope.chunk(byte);
            &chunk.as_bytes()[(byte - start_byte)..]
        },
        tree.as_ref(),
        None,
    );

    if let Some(new_tree) = new_tree {
        state.tree = Some(new_tree);
    } else {
        log.critical("tree-sitter::update_trees", "Failed to reparse main tree");

        // Return state
        state.tree = tree;

        return;
    }

    // Reload injected trees
    // We need to temporarily take ownership to avoid borrow conflicts
    let lang = state.lang.clone();
    let temp_state = TreeSitterState {
        lang: lang.clone(),
        parser: Parser::new(),
        tree: state.tree.clone(),
        injected_trees: vec![],
    };

    let injected_trees =
        load_injected_trees(&temp_state, &mut grammars, &config_path.0, &buf.rope, &log);

    // Update the injected trees
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

    let mut buf = buffers.cur_buffer_mut().await;

    // Ignore buffer if it's already open
    if buf.flags.contains("tree-sitter-checked") || buf.has_state::<TreeSitterState>() {
        return;
    }

    // Insert checked flag to prevent re-checks
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
            let (chunk, start_byte) = buf.rope.chunk(byte);
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
    };

    // Load injected trees using the initial state
    let injected_trees = load_injected_trees(
        &initial_state,
        &mut grammars,
        &config_path.0,
        &buf.rope,
        &log,
    );

    // Update state with injected trees
    buf.set_state(TreeSitterState {
        lang: initial_state.lang,
        parser: initial_state.parser,
        tree: initial_state.tree,
        injected_trees,
    });

    let state = buf
        .get_state_mut::<TreeSitterState>()
        .await
        .expect("State just inserted");

    let Some(highlighter) = Highlighter::new(&config_path.0, &mut grammars, &state, &buf.rope)
    else {
        return;
    };

    let spans = highlighter.collect_spans();

    buf.renderer.clear_extmark_ns("tree-sitter::highlights");

    for span in merge_overlapping_spans(spans) {
        let hl_style = translate_name_to_style(&theme, &span.capture_name);

        buf.add_extmark(
            ExtmarkBuilder::new_range("tree-sitter::highlights", span.byte_range.clone())
                .with_priority(span.priority as i32)
                .with_decoration(ExtmarkDecoration::Highlight { hl: hl_style }),
        );
    }
}

/// Loads all injected trees for a given parse tree
fn load_injected_trees(
    state: &TreeSitterState,
    grammars: &mut GrammarManager,
    config_path: &str,
    rope: &ropey::Rope,
    log: &LogSender,
) -> Vec<InjectedTree> {
    let mut injected_trees = Vec::new();

    // Try to load the injections query for this language
    let Some(injections_query) = grammars.get_query(config_path, &state.lang, "injections") else {
        // No injections query available, return empty vec
        return injected_trees;
    };

    // Capture indices for injection.language and injection.content
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
                // Try to extract the language name from the node text
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
                        log.critical(
                            "tree-sitter::load_injections",
                            format!("Failed to parse {} injection: {}", inj_lang, e),
                        );
                    }
                }
            }
        }

        // Continue walking
        true
    });

    injected_trees
}

/// Parses a single injected tree
fn parse_injection(
    grammars: &mut GrammarManager,
    config_path: &str,
    lang: &str,
    content_node: tree_sitter::Node,
    rope: &ropey::Rope,
) -> Result<InjectedTree, String> {
    // Load the grammar for the injected language
    let grammar = grammars
        .get_grammar(config_path, lang)
        .map_err(|e| format!("Failed to load grammar: {}", e))?;

    let mut parser = Parser::new();
    parser
        .set_language(&grammar.lang)
        .map_err(|e| format!("Failed to set language: {}", e))?;

    // Extract the content range
    let start_byte = content_node.start_byte();
    let end_byte = content_node.end_byte();

    // Parse the injected content
    let tree = parser
        .parse_with_options(
            &mut |byte, _| {
                let adjusted_byte = start_byte + byte;
                if adjusted_byte >= end_byte {
                    return &[] as &[u8];
                }
                let (chunk, chunk_start) = rope.chunk(adjusted_byte);
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
