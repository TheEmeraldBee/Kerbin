use kerbin_core::{kerbin_macros::State, *};
use tree_sitter::{InputEdit, Parser, QueryCursor, Range as TSRange, StreamingIterator};

use std::collections::{BTreeMap, HashMap};

pub mod util;
pub use util::*;

pub mod grammar;
pub use grammar::*;

pub mod state;
pub use state::*;

pub mod commands;
pub use commands::*;

pub mod highlight_string;

#[derive(State, Default)]
pub struct TreeSitterStates {
    pub bufs: BTreeMap<String, Option<TSState>>,
}

pub async fn register_lang(
    state: &mut State,
    name: impl ToString,
    exts: impl IntoIterator<Item = impl ToString>,
) {
    let name = name.to_string();

    for ext in exts.into_iter() {
        state
            .on_hook(hooks::UpdateFiletype::new(ext.to_string()))
            .system(render_tree_sitter_extmarks);

        state
            .lock_state::<GrammarManager>()
            .await
            .unwrap()
            .register_extension(ext, &name);
    }
}

pub async fn sync_buffer_changes_to_ts(
    states: ResMut<TreeSitterStates>,
    grammars: ResMut<GrammarManager>,
    buffers: ResMut<Buffers>,
) {
    get!(mut states, mut grammars, mut buffers);

    let mut buf = buffers.cur_buffer_mut().await;

    if !states.bufs.contains_key(&buf.path) {
        let ts_state = TSState::init(&buf.ext, &mut grammars);
        states.bufs.insert(buf.path.clone(), ts_state);
    }

    if let Some(Some(ts)) = states.bufs.get_mut(&buf.path)
        && !buf.byte_changes.is_empty()
    {
        ts.tree_sitter_dirty = true;
        ts.changes
            .extend(buf.byte_changes.iter().map(|change| InputEdit {
                start_position: tree_sitter::Point::new(change[0].0.0, change[0].0.1),
                start_byte: change[0].1,
                old_end_position: tree_sitter::Point::new(change[1].0.0, change[1].0.1),
                old_end_byte: change[1].1,
                new_end_position: tree_sitter::Point::new(change[2].0.0, change[2].0.1),
                new_end_byte: change[2].1,
            }));
        buf.byte_changes.clear();
    }
}

pub async fn parse_dirty_trees(
    states: ResMut<TreeSitterStates>,
    buffers: Res<Buffers>,
    grammars: ResMut<GrammarManager>,
) {
    get!(mut states, buffers, mut grammars);
    let buf = buffers.cur_buffer().await;

    if let Some(Some(ts_state)) = states.bufs.get_mut(&buf.path) {
        if !ts_state.tree_sitter_dirty {
            return;
        }

        if let Some(tree) = &mut ts_state.primary_tree {
            for edit in &ts_state.changes {
                tree.edit(edit);
            }
        }

        ts_state.primary_tree = ts_state.parser.parse_with_options(
            &mut |byte, _| {
                let (chunk, start_byte) = buf.rope.chunk(byte);
                &chunk.as_bytes()[(byte - start_byte)..]
            },
            ts_state.primary_tree.as_ref(),
            None,
        );

        if let Some(primary_tree) = ts_state.primary_tree.as_ref()
            && let Some(injection_query) = grammars.get_query(&ts_state.language_name, "injections")
        {
            let mut new_ranges_by_lang: HashMap<String, Vec<TSRange>> = HashMap::new();
            let mut query_cursor = QueryCursor::new();
            let provider = TextProviderRope(&buf.rope);
            let mut matches =
                query_cursor.matches(&injection_query, primary_tree.root_node(), &provider);

            while let Some(m) = matches.next() {
                let mut content_node = None;
                let mut lang_name = injection_query
                    .property_settings(m.pattern_index)
                    .iter()
                    .find(|prop| prop.key.as_ref() == "injection.language")
                    .and_then(|prop| prop.value.as_ref().map(|x| x.to_string()));

                for cap in m.captures {
                    if injection_query.capture_names()[cap.index as usize] == "injection.content" {
                        content_node = Some(cap.node);
                    } else if injection_query.capture_names()[cap.index as usize]
                        == "injection.language"
                    {
                        lang_name = Some(buf.rope.slice(cap.node.byte_range()).to_string());
                    }
                }

                if lang_name.is_none() {
                    continue;
                }

                if let (Some(content), Some(lang)) = (content_node, lang_name) {
                    new_ranges_by_lang
                        .entry(lang)
                        .or_default()
                        .push(content.range());
                }
            }

            let mut old_parsers = std::mem::take(&mut ts_state.injected_parsers);
            let mut new_parsers = HashMap::new();

            for (lang, ranges) in new_ranges_by_lang {
                let (mut parser, mut tree) = old_parsers.remove(&lang).unwrap_or_else(|| {
                    let mut new_parser = Parser::new();
                    if let Some(language) = grammars.get_language(&lang) {
                        new_parser.set_language(&language).unwrap();
                    } else {
                        tracing::error!("Couldn't find `{lang}` in tree-sitter grammars");
                    }
                    (new_parser, None)
                });

                parser.set_included_ranges(&ranges).unwrap();

                if let Some(t) = &mut tree {
                    for edit in &ts_state.changes {
                        t.edit(edit);
                    }
                }

                let new_tree = parser.parse_with_options(
                    &mut |byte, _| {
                        let (chunk, start_byte) = buf.rope.chunk(byte);
                        &chunk.as_bytes()[(byte - start_byte)..]
                    },
                    tree.as_ref(),
                    None,
                );
                new_parsers.insert(lang, (parser, new_tree));
            }
            ts_state.injected_parsers = new_parsers;
        }

        ts_state.changes.clear();
        ts_state.tree_sitter_dirty = false;
    }
}

pub async fn calculate_highlights(
    ts_states: Res<TreeSitterStates>,
    grammars: ResMut<GrammarManager>,
    buffers: Res<Buffers>,
    theme: Res<Theme>,
    highlights: ResMut<HighlightMap>,
) {
    get!(ts_states, mut grammars, buffers, theme, mut highlights);

    let buf = buffers.cur_buffer().await;

    if let Some(Some(ts_state)) = ts_states.bufs.get(&buf.path) {
        let mut final_highlights = BTreeMap::new();

        if let Some(tree) = ts_state.primary_tree.as_ref()
            && let Some(query) = grammars.get_query(&ts_state.language_name, "highlight")
        {
            final_highlights.extend(highlight(&buf.rope, tree, &query, &theme));
        }

        for (lang_name, (_, tree_opt)) in &ts_state.injected_parsers {
            if let Some(tree) = tree_opt
                && let Some(query) = grammars.get_query(lang_name, "highlight")
            {
                final_highlights.extend(highlight(&buf.rope, tree, &query, &theme));
            }
        }
        highlights.0.insert(buf.path.clone(), final_highlights);
    }
}

pub async fn init(state: &mut State) {
    state
        .state(TreeSitterStates::default())
        .state(GrammarManager::default())
        .state(HighlightMap::default());

    state
        .on_hook(hooks::PostUpdate)
        .system(sync_buffer_changes_to_ts)
        .system(parse_dirty_trees)
        .system(calculate_highlights);

    {
        let mut commands = state.lock_state::<CommandRegistry>().await.unwrap();
        commands.register::<TSCommand>();
    }
}
