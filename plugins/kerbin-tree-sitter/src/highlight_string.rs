use ropey::Rope;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::{GrammarManager, TextProviderRope, highlight};
use kerbin_core::{Theme, ascii_forge::window::ContentStyle};

/// Represents a line with styled segments
#[derive(Clone, Debug)]
pub struct StyledLine {
    pub segments: Vec<(String, ContentStyle)>,
}

impl StyledLine {
    pub fn new() -> Self {
        Self { segments: vec![] }
    }

    pub fn push(&mut self, text: String, style: ContentStyle) {
        self.segments.push((text, style));
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty() || self.segments.iter().all(|(s, _)| s.is_empty())
    }

    pub fn width(&self) -> usize {
        self.segments.iter().map(|(s, _)| s.len()).sum()
    }
}

/// Highlight a string with tree-sitter for a given language
pub fn highlight_string(
    text: &str,
    language: &str,
    grammars: &mut GrammarManager,
    theme: &Theme,
) -> Vec<StyledLine> {
    // Get the language and query
    let Some(lang) = grammars.get_language(language) else {
        // Fallback to unstyled text
        return fallback_unstyled(text, theme);
    };

    let Some(query) = grammars.get_query(language, "highlight") else {
        // Fallback to unstyled text
        return fallback_unstyled(text, theme);
    };

    // Parse the string
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return fallback_unstyled(text, theme);
    }

    let rope = Rope::from_str(text);
    let tree = parser.parse_with_options(
        &mut |byte, _| {
            let (chunk, start_byte) = rope.chunk(byte);
            &chunk.as_bytes()[(byte - start_byte)..]
        },
        None,
        None,
    );

    let Some(tree) = tree else {
        return fallback_unstyled(text, theme);
    };

    // Get highlights
    let highlights = highlight(&rope, &tree, &query, theme);

    // Convert to styled lines
    convert_highlights_to_styled_lines(&rope, &highlights, theme)
}

/// Highlight markdown with code block injection support
pub fn highlight_markdown(
    text: &str,
    default_lang: &str,
    grammars: &mut GrammarManager,
    theme: &Theme,
) -> Vec<StyledLine> {
    // Get markdown language and query
    let Some(md_lang) = grammars.get_language("markdown") else {
        return fallback_unstyled(text, theme);
    };

    let Some(md_query) = grammars.get_query("markdown", "highlight") else {
        return fallback_unstyled(text, theme);
    };

    // Parse the markdown
    let mut parser = Parser::new();
    if parser.set_language(&md_lang).is_err() {
        return fallback_unstyled(text, theme);
    }

    let rope = Rope::from_str(text);
    let tree = parser.parse_with_options(
        &mut |byte, _| {
            let (chunk, start_byte) = rope.chunk(byte);
            &chunk.as_bytes()[(byte - start_byte)..]
        },
        None,
        None,
    );

    let Some(tree) = tree else {
        return fallback_unstyled(text, theme);
    };

    // Get base markdown highlights
    let mut all_highlights = highlight(&rope, &tree, &md_query, theme);

    // Find and highlight code blocks with injection
    if let Some(injection_query) = grammars.get_query("markdown", "injections") {
        let injected_highlights = process_markdown_injections(
            &rope,
            &tree,
            default_lang,
            &injection_query,
            grammars,
            theme,
        );

        // Merge injected highlights (they have higher priority)
        all_highlights.extend(injected_highlights);
    }

    // Convert to styled lines
    convert_highlights_to_styled_lines(&rope, &all_highlights, theme)
}

/// Process markdown code block injections
fn process_markdown_injections(
    rope: &Rope,
    tree: &tree_sitter::Tree,
    default_lang: &str,
    injection_query: &Query,
    grammars: &mut GrammarManager,
    theme: &Theme,
) -> std::collections::BTreeMap<usize, ContentStyle> {
    let mut highlights = std::collections::BTreeMap::new();

    let mut query_cursor = QueryCursor::new();
    let provider = TextProviderRope(rope);
    let mut matches = query_cursor.matches(injection_query, tree.root_node(), &provider);

    while let Some(m) = matches.next() {
        let mut content_node = None;
        let mut lang_name = default_lang.to_string();

        for cap in m.captures {
            let name = injection_query.capture_names()[cap.index as usize];

            if name == "injection.content" {
                content_node = Some(cap.node);
            } else if name == "injection.language" {
                lang_name = rope
                    .slice(cap.node.byte_range())
                    .to_string()
                    .trim()
                    .to_string();
            }
        }

        if let (Some(content), lang) = (content_node, lang_name) {
            // Get the language parser and query
            if let Some(lang_obj) = grammars.get_language(&lang)
                && let Some(lang_query) = grammars.get_query(&lang, "highlight")
            {
                // Parse the code block content
                let mut parser = Parser::new();
                if parser.set_language(&lang_obj).is_ok() {
                    let range = content.byte_range();
                    let code_text = rope.slice(range.clone()).to_string();
                    let code_rope = Rope::from_str(&code_text);

                    if let Some(code_tree) = parser.parse_with_options(
                        &mut |byte, _| {
                            let (chunk, start_byte) = code_rope.chunk(byte);
                            &chunk.as_bytes()[(byte - start_byte)..]
                        },
                        None,
                        None,
                    ) {
                        // Get highlights for the code block
                        let code_highlights = highlight(&code_rope, &code_tree, &lang_query, theme);

                        // Offset the highlights to match the original rope
                        for (offset, style) in code_highlights {
                            highlights.insert(range.start + offset, style);
                        }
                    }
                }
            }
        }
    }

    highlights
}

/// Convert BTreeMap highlights to styled lines
fn convert_highlights_to_styled_lines(
    rope: &Rope,
    highlights: &std::collections::BTreeMap<usize, ContentStyle>,
    theme: &Theme,
) -> Vec<StyledLine> {
    use ropey::LineType;

    let default_style = theme.get_fallback_default(["ui.text"]);
    let mut lines = vec![];

    let total_lines = rope.len_lines(LineType::LF_CR);

    for line_idx in 0..total_lines {
        let line_start = rope.line_to_byte_idx(line_idx, LineType::LF_CR);
        let line_end = if line_idx + 1 < total_lines {
            rope.line_to_byte_idx(line_idx + 1, LineType::LF_CR)
        } else {
            rope.len()
        };

        let mut styled_line = StyledLine::new();
        let mut current_pos = line_start;
        let mut current_style = default_style;

        // Get the initial style for this line
        if let Some((_, &style)) = highlights.range(..=line_start).next_back() {
            current_style = style;
        }

        // Collect all style changes on this line
        let mut style_changes: Vec<(usize, ContentStyle)> = highlights
            .range(line_start..line_end)
            .map(|(&pos, &style)| (pos, style))
            .collect();

        // Add end marker
        style_changes.push((line_end, default_style));

        for (pos, style) in style_changes {
            if pos > current_pos {
                // Add segment with current style
                let text = rope.slice(current_pos..pos.min(line_end)).to_string();
                // Remove trailing newline
                let text = text.trim_end_matches('\n').trim_end_matches('\r');
                if !text.is_empty() {
                    styled_line.push(text.to_string(), current_style);
                }
                current_pos = pos;
            }
            current_style = style;
        }

        // If the line is empty, add a placeholder
        if styled_line.segments.is_empty() {
            styled_line.push(String::new(), default_style);
        }

        lines.push(styled_line);
    }

    lines
}

/// Fallback for unstyled text
fn fallback_unstyled(text: &str, theme: &Theme) -> Vec<StyledLine> {
    let default_style = theme.get_fallback_default(["ui.text"]);

    text.lines()
        .map(|line| {
            let mut styled_line = StyledLine::new();
            styled_line.push(line.to_string(), default_style);
            styled_line
        })
        .collect()
}
