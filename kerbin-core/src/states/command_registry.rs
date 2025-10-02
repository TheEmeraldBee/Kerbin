use ascii_forge::window::Buffer;

use crate::*;

/// State used for storing registered commands within the system itself.
///
/// Also used for parsing and validating commands.
#[derive(State)]
pub struct CommandRegistry(pub Vec<RegisteredCommandSet>);

impl CommandRegistry {
    /// Registers the given command type within the editor using its implementation.
    ///
    /// The command type must implement the `AsCommandInfo` trait to provide command metadata.
    ///
    /// # Type Parameters
    ///
    /// * `T`: The command type that implements `AsCommandInfo`.
    pub fn register<T: AsCommandInfo + 'static>(&mut self) {
        self.0.push(RegisteredCommandSet {
            parser: Box::new(T::from_str),
            infos: T::infos(),
        })
    }

    /// Splits the command's string using `shellwords` in order to handle commands
    /// closer to how a shell does. Command parser and validator do this automatically.
    ///
    /// This function handles quoted strings and other shell-like parsing rules.
    ///
    /// # Arguments
    ///
    /// * `input`: The command string to split.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the split words of the command. Returns a vector
    /// with the original input as a single string if splitting fails.
    pub fn split_command(input: &str) -> Vec<String> {
        shellwords::split(input).unwrap_or(vec![input.to_string()])
    }

    /// Given the input, will return whether the input would be considered a valid command.
    ///
    /// This method attempts to parse the command without logging errors, checking against
    /// registered commands and applying command prefixes if applicable.
    ///
    /// # Arguments
    ///
    /// * `input`: The input string to validate.
    /// * `prefix_registry`: A reference to the `CommandPrefixRegistry` for checking against
    ///   registered command prefixes.
    /// * `modes`: A reference to the `ModeStack` to determine active modes for
    ///   mode-specific command validation.
    ///
    /// # Returns
    ///
    /// `true` if the input can be successfully parsed into a command, `false` otherwise.
    pub fn validate_command(
        &self,
        input: &str,
        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> bool {
        self.parse_command(
            Self::split_command(input),
            false, // Do not log errors during validation
            true,  // Indicate that prefix checking should happen (or has happened)
            prefix_registry,
            modes,
        )
        .is_some()
    }

    /// Retrieves the buffers used for handling the rendering of
    /// command palette within the core engine.
    /// Buffers are used so theming can be stored directly.
    ///
    /// This method takes an input string, attempts to find matching commands, ranks them
    /// by relevance, and generates `Buffer` representations for display, along with
    /// an optional completion string and a description buffer for the top suggestion.
    ///
    /// # Arguments
    ///
    /// * `input`: The current input string entered into the command palette.
    /// * `theme`: The current `Theme` to apply styling to the suggestion buffers.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - `Vec<Buffer>`: A vector of `Buffer` instances, each representing a command suggestion.
    ///   The top suggestion is styled differently if a completion is available.
    /// - `Option<String>`: An optional string representing the auto-completion for the
    ///   first word of the input if there's a clear top suggestion.
    /// - `Option<Buffer>`: An optional `Buffer` containing the detailed description
    ///   of the top command suggestion.
    pub fn get_command_suggestions(
        &self,
        input: &str,

        theme: &Theme,
    ) -> (Vec<Buffer>, Option<String>, Option<Buffer>) {
        let words = shellwords::split(input).unwrap_or(vec![input.to_string()]);

        if words.is_empty() {
            return (vec![], None, None);
        }

        let mut res = vec![];

        for registry in &self.0 {
            for info in &registry.infos {
                for valid_name in &info.valid_names {
                    let Some(rnk) = rank(&words[0], valid_name) else {
                        continue;
                    };

                    res.push((rnk, info, valid_name.to_string()));
                    break;
                }
            }
        }

        res.sort_by(|l, r| l.0.cmp(&r.0));

        let desc = res.first().and_then(|x| x.1.desc_buf(theme));

        let completion = if words.len() == 1 {
            res.first().map(|x| x.2.clone())
        } else {
            None
        };

        (
            res.iter()
                .enumerate()
                .map(|(i, x)| {
                    if i == 0 && completion.is_some() {
                        x.1.as_suggestion_with_search(true, input, theme)
                    } else {
                        x.1.as_suggestion_with_search(false, input, theme)
                    }
                })
                .collect(),
            completion,
            desc,
        )
    }

    /// Parses a given list of command words into a `Box<dyn Command>`.
    ///
    /// This is the core method for converting user input into executable commands. It handles
    /// command prefixes based on the current mode stack and attempts to parse the words
    /// against all registered command parsers.
    ///
    /// # Arguments
    ///
    /// * `words`: A `Vec<String>` representing the command and its arguments. This vector
    ///   might be modified if command prefixes are applied.
    /// * `log_errors`: If `true`, parsing errors (returned by command parsers) will be
    ///   logged using `tracing::error!`.
    /// * `prefix_checked`: If `true`, the command prefix application logic will be skipped.
    ///   This is useful if the command has already been pre-processed.
    /// * `prefix_registry`: A reference to the `CommandPrefixRegistry` used to find and
    ///   apply command prefixes based on the current mode.
    /// * `modes`: A reference to the `ModeStack` to determine the currently active modes,
    ///   which influence which command prefixes are applied.
    ///
    /// # Returns
    ///
    /// An `Option<Box<dyn Command>>` containing the parsed and boxed command if successful.
    /// Returns `None` if the input cannot be parsed into a valid command, or if an error
    /// occurs during parsing and `log_errors` is true.
    pub fn parse_command(
        &self,
        mut words: Vec<String>,
        log_errors: bool,
        prefix_checked: bool,

        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> Option<Box<dyn Command>> {
        if !prefix_checked {
            for prefix in &prefix_registry.0 {
                if prefix.modes.iter().any(|x| modes.mode_on_stack(*x)) {
                    // Check that the cmd name is valid
                    let mut has_name = false;
                    if !prefix.list.is_empty() {
                        for infos in &self.0 {
                            if infos.infos.iter().any(|x| {
                                let matches_word0 = x.check_name(&words[0]);
                                let matches_prefix = prefix.list.iter().any(|l| x.check_name(l));
                                matches_word0 && matches_prefix
                            }) {
                                has_name = true;
                            }

                            if has_name {
                                break;
                            }
                        }
                    } else {
                        has_name = true
                    }

                    if prefix.include != has_name {
                        continue;
                    }

                    // Prefix the command with the resulting prefix split
                    let mut new_words = Self::split_command(&prefix.prefix_cmd);
                    new_words.append(&mut words);

                    words = new_words;
                }
            }
        }

        if words.is_empty() {
            return None;
        }

        for registry in &self.0 {
            if let Some(cmd) = (registry.parser)(&words) {
                match cmd {
                    Ok(t) => return Some(t),
                    Err(e) => {
                        if log_errors {
                            tracing::error!("Failed to parse command due to: {e:?}");
                        }
                        return None;
                    }
                }
            }
        }
        None
    }
}
