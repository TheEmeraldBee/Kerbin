use crate::*;
use ratatui::prelude::*;

use crate::Theme;

mod state;
pub use state::*;

mod buffer;
pub use buffer::*;

mod mode;
pub use mode::*;

mod shell;
pub use shell::*;

mod motion;
pub use motion::*;

mod cursor;
pub use cursor::*;

mod palette;
pub use palette::*;

mod registers;
pub use registers::*;

mod input;
pub use input::*;

mod config;
pub use config::*;

/// Type alias for a command parsing function.
type CommandFn = Box<dyn Fn(&[Token]) -> Option<Result<Box<dyn Command>, String>> + Send + Sync>;

/// Represents a set of registered commands, including its parser and command information.
pub struct RegisteredCommandSet {
    /// The function responsible for parsing a list of string arguments into a command.
    pub parser: CommandFn,
    /// A vector of `CommandInfo` structs, providing metadata for the commands handled by this parser.
    pub infos: Vec<CommandInfo>,
}

/// Represents a command prefix configuration.
#[derive(Debug)]
pub struct CommandPrefix {
    pub modes: Vec<char>,
    pub prefix_cmd: String,
    pub include: bool,
    pub list: Vec<String>,
}

/// An applyable command that will change the whole state in some way
#[async_trait::async_trait]
pub trait Command: Send + Sync {
    async fn apply(&self, state: &mut State) -> bool;
}

/// The text info of what a command expects and uses
#[derive(Debug)]
pub struct CommandInfo {
    pub valid_names: Vec<String>,
    pub args: Vec<(String, String)>,
    pub desc: Vec<String>,
}

impl CommandInfo {
    pub fn new(
        names: impl IntoIterator<Item = impl ToString>,
        args: impl IntoIterator<Item = (impl ToString, impl ToString)>,
        desc: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        Self {
            valid_names: names.into_iter().map(|x| x.to_string()).collect(),
            args: args
                .into_iter()
                .map(|x| (x.0.to_string(), x.1.to_string()))
                .collect(),
            desc: desc.into_iter().map(|x| x.to_string()).collect(),
        }
    }

    pub fn check_name(&self, name: impl ToString) -> bool {
        self.valid_names.contains(&name.to_string())
    }

    /// Returns a `Line` with the description text, or `None` if empty.
    pub fn desc_buf(&self, theme: &Theme) -> Option<Vec<Line<'static>>> {
        if self.desc.is_empty() {
            return None;
        }

        let desc_style = theme.get_fallback_default(["ui.commandline.desc", "ui.text"]);

        Some(
            self.desc
                .iter()
                .map(|text| Line::from(Span::styled(text.clone(), desc_style)))
                .collect(),
        )
    }

    /// Returns a suggestion `Line` for this command
    pub fn as_suggestion(&self, will_autocomplete: bool, theme: &Theme) -> Line<'static> {
        self.as_suggestion_with_search(will_autocomplete, "", theme)
    }

    /// Returns a suggestion `Line` with search term highlighting
    pub fn as_suggestion_with_search(
        &self,
        will_autocomplete: bool,
        search_term: &str,
        theme: &Theme,
    ) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();

        let primary_name = &self.valid_names[0];

        let base_name_style = theme.get_fallback_default(if will_autocomplete {
            [
                "ui.commandline.auto_name",
                "ui.commandline.primary_name",
                "ui.commandline.names",
                "ui.text",
            ]
            .as_slice()
        } else {
            [
                "ui.commandline.primary_name",
                "ui.commandline.names",
                "ui.text",
            ]
            .as_slice()
        });

        let match_highlight_style = theme.get_fallback_default([
            "ui.commandline.match_highlight",
            "ui.commandline.primary_name",
            "ui.text",
        ]);

        if !search_term.is_empty() {
            spans.extend(highlight_matches(
                primary_name,
                search_term,
                base_name_style,
                match_highlight_style,
            ));
        } else {
            spans.push(Span::styled(primary_name.clone(), base_name_style));
        }
        spans.push(Span::raw(" "));

        if self.valid_names.len() >= 2 {
            let alias_style = theme.get_fallback_default([
                "ui.commandline.alias",
                "ui.commandline.names",
                "ui.text",
            ]);
            let bracket_style =
                theme.get_fallback_default(["ui.commandline.bracket", "ui.text"]);

            spans.push(Span::styled("(", bracket_style));
            spans.push(Span::styled(
                self.valid_names[1..].join(", "),
                alias_style,
            ));
            spans.push(Span::styled(") ", bracket_style));
        }

        let name_style = theme.get_fallback_default(["ui.commandline.arg_name", "ui.text"]);
        let type_style = theme.get_fallback_default([
            "ui.commandline.arg_type",
            "ui.commandline.arg_name",
            "ui.text",
        ]);
        let separator_style =
            theme.get_fallback_default(["ui.commandline.arg_separator", "ui.text"]);

        for (i, (name, ty)) in self.args.iter().enumerate() {
            spans.push(Span::styled(name.clone(), name_style));
            spans.push(Span::styled(":", separator_style));
            spans.push(Span::styled(ty.clone(), type_style));
            if i < self.args.len() - 1 {
                spans.push(Span::raw(" "));
            }
        }

        Line::from(spans)
    }
}

/// Highlights matching characters in text based on search term
fn highlight_matches(
    text: &str,
    search: &str,
    base_style: Style,
    highlight_style: Style,
) -> Vec<Span<'static>> {
    let mut result = Vec::new();

    if search.is_empty() {
        result.push(Span::styled(text.to_string(), base_style));
        return result;
    }

    let text_lower = text.to_lowercase();
    let search_lower = search.to_lowercase();
    let text_chars: Vec<char> = text.chars().collect();
    let search_chars: Vec<char> = search_lower.chars().collect();

    let mut search_idx = 0;
    let mut current_segment = String::new();
    let mut current_style = base_style;
    let mut is_matching = false;

    for (i, ch) in text_lower.chars().enumerate() {
        let should_highlight = search_idx < search_chars.len() && ch == search_chars[search_idx];

        if should_highlight {
            if !is_matching && !current_segment.is_empty() {
                result.push(Span::styled(current_segment.clone(), current_style));
                current_segment.clear();
            }
            current_style = highlight_style;
            is_matching = true;
            current_segment.push(text_chars[i]);
            search_idx += 1;
        } else {
            if is_matching && !current_segment.is_empty() {
                result.push(Span::styled(current_segment.clone(), current_style));
                current_segment.clear();
            }
            current_style = base_style;
            is_matching = false;
            current_segment.push(text_chars[i]);
        }
    }

    if !current_segment.is_empty() {
        result.push(Span::styled(current_segment, current_style));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_matches_basic() {
        let base = Style::default();
        let highlight = Style::default().bold();

        let result = highlight_matches("hello", "hlo", base, highlight);

        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_highlight_matches_empty_search() {
        let base = Style::default();
        let highlight = Style::default().bold();

        let result = highlight_matches("hello", "", base, highlight);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_highlight_matches_no_match() {
        let base = Style::default();
        let highlight = Style::default().bold();

        let result = highlight_matches("hello", "xyz", base, highlight);

        assert_eq!(result.len(), 1);
    }
}

/// This trait will allow you to use commands from 'c' mode.
pub trait AsCommandInfo: Command + CommandFromStr {
    fn infos() -> Vec<CommandInfo>;
}

/// This trait should be implemented on anything you want to be able to define within a config
pub trait CommandFromStr: Command {
    fn from_str(val: &[Token]) -> Option<Result<Box<dyn Command>, String>>;
}
