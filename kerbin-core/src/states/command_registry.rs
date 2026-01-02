use ascii_forge::window::Buffer;

use crate::*;

/// State for storing registered commands and parsing input
#[derive(State)]
pub struct CommandRegistry(pub Vec<RegisteredCommandSet>);

impl CommandRegistry {
    /// Registers a command type within the editor
    pub fn register<T: AsCommandInfo + 'static>(&mut self) {
        self.0.push(RegisteredCommandSet {
            parser: Box::new(T::from_str),
            infos: T::infos(),
        })
    }

    /// Determines if the input string represents a valid command
    pub fn validate_command(
        &self,
        input: &str,
        resolver: Option<&Resolver<'_>>,

        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> bool {
        self.parse_command(
            word_split(input),
            false, // Do not log errors during validation
            true,  // Indicate that prefix checking should happen (or has happened)
            resolver,
            false,
            prefix_registry,
            modes,
        )
        .is_some()
    }

    /// Retrieves command suggestions and theming for the palette
    pub async fn get_command_suggestions(
        &self,
        input: &str,

        theme: &Theme,
    ) -> (Vec<Buffer>, Option<String>, Option<Buffer>) {
        let resolver = resolver_engine().await;
        resolver.as_resolver().expand_str(input, false);

        let words = word_split(input);

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

    #[allow(clippy::too_many_arguments)]
    /// Parses a list of words into a runnable command
    pub fn parse_command(
        &self,
        mut words: Vec<String>,
        log_errors: bool,
        prefix_checked: bool,

        resolver: Option<&Resolver<'_>>,
        allow_run: bool,

        prefix_registry: &CommandPrefixRegistry,
        modes: &ModeStack,
    ) -> Option<Box<dyn Command>> {
        if let Some(resolver) = resolver {
            words = words
                .iter()
                .map(|x| resolver.expand_str(x, allow_run))
                .collect();
        }

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
                    let mut new_words = word_split(&prefix.prefix_cmd);
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
