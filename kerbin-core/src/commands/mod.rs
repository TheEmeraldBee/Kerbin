use ascii_forge::prelude::*;
use kerbin_state_machine::State;

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

/// Type alias for a command parsing function.
///
/// This defines the signature required for functions that can parse a slice of
/// strings (command words) into an `Option` of `Result` containing a boxed `Command`.
type CommandFn = Box<dyn Fn(&[String]) -> Option<Result<Box<dyn Command>, String>> + Send + Sync>;

/// Represents a set of registered commands, including its parser and command information.
///
/// Each `RegisteredCommandSet` groups a parser function with the metadata
/// (`CommandInfo`) for the commands it can parse.
pub struct RegisteredCommandSet {
    /// The function responsible for parsing a list of string arguments into a command.
    pub parser: CommandFn,
    /// A vector of `CommandInfo` structs, providing metadata for the commands handled by this parser.
    pub infos: Vec<CommandInfo>,
}

/// Represents a command prefix configuration.
///
/// Command prefixes allow automatically prepending specific commands or arguments
/// to user input based on the active editor mode.
#[derive(Debug)]
pub struct CommandPrefix {
    /// A list of character codes representing the modes in which this prefix should be active.
    /// If any of these modes are on the `ModeStack`, the prefix logic will be applied.
    pub modes: Vec<char>,
    /// The command string that will be prepended to the user's input. This string
    /// is split into words using `shellwords::split` before prepending.
    pub prefix_cmd: String,

    /// A boolean indicating whether the `list` acts as an `include` filter (`true`)
    /// or an `exclude` filter (`false`, default) for command names.
    pub include: bool,
    /// Depending on the `include` flag, this is either an inclusion list (only commands
    /// in this list are prefixed) or an exclusion list (commands in this list are NOT prefixed).
    pub list: Vec<String>,
}

/// An applyable command that will change the whole state in some way
pub trait Command: Send + Sync {
    fn apply(&self, state: &mut State) -> bool;
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

    /// Checks if the name passed is valid in the CommandInfo's eyes
    pub fn check_name(&self, name: impl ToString) -> bool {
        self.valid_names.contains(&name.to_string())
    }

    /// Returns a optional buffer that contains the description of the command
    /// Is None if the description of the command is empty
    pub fn desc_buf(&self, theme: &Theme) -> Option<Buffer> {
        if self.desc.is_empty() {
            return None;
        }

        let mut buf = Buffer::new((100, 100));

        let desc_style = theme.get_fallback_default(["ui.commandline.desc", "ui.text"]);

        for (i, text) in self.desc.iter().enumerate() {
            render!(buf, (0, i as u16) => [desc_style.apply(&text)]);
        }

        buf.shrink();

        Some(buf)
    }

    /// Returns a suggestion line with enhanced noice.nvim-style formatting
    /// and optional match highlighting
    pub fn as_suggestion(&self, will_autocomplete: bool, theme: &Theme) -> Buffer {
        self.as_suggestion_with_search(will_autocomplete, "", theme)
    }

    /// Returns a suggestion line with search term highlighting
    pub fn as_suggestion_with_search(
        &self,
        will_autocomplete: bool,
        search_term: &str,
        theme: &Theme,
    ) -> Buffer {
        let mut buf = Buffer::new((500, 1));

        // Get the primary command name
        let primary_name = &self.valid_names[0];

        // Determine name style based on autocomplete state
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

        // Render primary name with highlighting if search term provided
        let mut loc = if !search_term.is_empty() {
            let highlighted = highlight_matches(
                primary_name,
                search_term,
                base_name_style,
                match_highlight_style,
            );
            let mut pos = vec2(0, 0);
            for styled in highlighted {
                pos = render!(buf, pos => [styled]);
            }
            render!(buf, pos => [" "])
        } else {
            render!(buf, (0, 0) => [
                StyledContent::new(base_name_style, primary_name),
                " "
            ])
        };

        // Add aliases if present
        if self.valid_names.len() >= 2 {
            let alias_style = theme.get_fallback_default([
                "ui.commandline.alias",
                "ui.commandline.names",
                "ui.text",
            ]);

            let bracket_style = theme.get_fallback_default(["ui.commandline.bracket", "ui.text"]);

            loc = render!(buf, loc => [
                StyledContent::new(bracket_style, "("),
                StyledContent::new(alias_style, self.valid_names[1..].join(", ")),
                StyledContent::new(bracket_style, ") ")
            ]);
        }

        // Render arguments with enhanced styling
        let name_style = theme.get_fallback_default(["ui.commandline.arg_name", "ui.text"]);

        let type_style = theme.get_fallback_default([
            "ui.commandline.arg_type",
            "ui.commandline.arg_name",
            "ui.text",
        ]);

        let separator_style =
            theme.get_fallback_default(["ui.commandline.arg_separator", "ui.text"]);

        for (i, (name, ty)) in self.args.iter().enumerate() {
            loc = render!(buf, loc => [
                StyledContent::new(name_style, name),
                StyledContent::new(separator_style, ":"),
                StyledContent::new(type_style, ty),
            ]);

            // Add spacing between args
            if i < self.args.len() - 1 {
                loc = render!(buf, loc => [" "]);
            }
        }

        buf.shrink();
        buf
    }
}

/// Highlights matching characters in text based on search term
///
/// This function finds all characters from the search term in the text
/// (in order, but not necessarily consecutive) and applies highlight styling
/// to matching characters while preserving the base style for non-matching ones.
fn highlight_matches(
    text: &str,
    search: &str,
    base_style: ContentStyle,
    highlight_style: ContentStyle,
) -> Vec<StyledContent<String>> {
    let mut result = Vec::new();

    if search.is_empty() {
        result.push(StyledContent::new(base_style, text.to_string()));
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
            // Flush previous segment if style changed
            if !is_matching && !current_segment.is_empty() {
                result.push(StyledContent::new(current_style, current_segment.clone()));
                current_segment.clear();
            }

            current_style = highlight_style;
            is_matching = true;
            current_segment.push(text_chars[i]);
            search_idx += 1;
        } else {
            // Flush previous segment if style changed
            if is_matching && !current_segment.is_empty() {
                result.push(StyledContent::new(current_style, current_segment.clone()));
                current_segment.clear();
            }

            current_style = base_style;
            is_matching = false;
            current_segment.push(text_chars[i]);
        }
    }

    // Flush final segment
    if !current_segment.is_empty() {
        result.push(StyledContent::new(current_style, current_segment));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_matches_basic() {
        let base = ContentStyle::default();
        let highlight = ContentStyle::default().bold();

        let result = highlight_matches("hello", "hlo", base, highlight);

        // Should highlight h, l, o
        assert_eq!(result.len(), 5); // h, e, l, l, o alternating styles
    }

    #[test]
    fn test_highlight_matches_empty_search() {
        let base = ContentStyle::default();
        let highlight = ContentStyle::default().bold();

        let result = highlight_matches("hello", "", base, highlight);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_highlight_matches_no_match() {
        let base = ContentStyle::default();
        let highlight = ContentStyle::default().bold();

        let result = highlight_matches("hello", "xyz", base, highlight);

        // Should not highlight anything
        assert_eq!(result.len(), 1);
    }
}

/// This trait will allow you to use commands from 'c' mode. This will give you verification info,
/// as well as argument expectations and types. This shouldn't need to be implemented manually.
/// Just use the #[derive(Command)] and the additional attributes on the struct.
pub trait AsCommandInfo: Command + CommandFromStr {
    fn infos() -> Vec<CommandInfo>;
}

/// This trait should be implemented on anything you want to be able to define within a config
/// This will turn the command into an executable command based on the string input.
/// Used for config, as well as the command pallette Serde + a parsing library can make this much
/// easier to implement
pub trait CommandFromStr: Command {
    fn from_str(val: &[String]) -> Option<Result<Box<dyn Command>, String>>;
}
