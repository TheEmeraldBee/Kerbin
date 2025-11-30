#[derive(Debug, Clone, Copy, PartialEq)]
enum ParseState {
    Normal,
    InDoubleQuotes,
    InSingleQuotes,
    Escaped,
    DollarSeen,
    InCommandSubstitution,
}

pub fn word_split(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut state = ParseState::Normal;
    let mut paren_depth = 0;

    for ch in input.chars() {
        state = match (state, ch) {
            // Handle escaping in various states (but NOT in single quotes)
            (ParseState::Escaped, _) => {
                current.push(ch);
                ParseState::Normal
            }
            (ParseState::Normal, '\\') => ParseState::Escaped,
            (ParseState::InDoubleQuotes, '\\') => ParseState::Escaped,

            // Quote handling - must come before whitespace handling
            (ParseState::Normal, '"') => ParseState::InDoubleQuotes,
            (ParseState::Normal, '\'') => ParseState::InSingleQuotes,
            (ParseState::InDoubleQuotes, '"') => ParseState::Normal,
            (ParseState::InSingleQuotes, '\'') => ParseState::Normal,

            // Inside single quotes - everything is literal, including backslashes
            (ParseState::InSingleQuotes, _) => {
                current.push(ch);
                ParseState::InSingleQuotes
            }

            // Inside double quotes - preserve spaces
            (ParseState::InDoubleQuotes, _) => {
                current.push(ch);
                ParseState::InDoubleQuotes
            }

            // Handle dollar sign for command substitution
            (ParseState::Normal, '$') => ParseState::DollarSeen,
            (ParseState::InDoubleQuotes, '$') => ParseState::DollarSeen,

            // Start command substitution
            (ParseState::DollarSeen, '(') => {
                current.push('$');
                current.push('(');
                paren_depth = 1;
                ParseState::InCommandSubstitution
            }

            // Dollar not followed by paren - just regular dollar
            (ParseState::DollarSeen, _) => {
                current.push('$');
                current.push(ch);
                ParseState::Normal
            }

            // Inside command substitution - track nested parens
            (ParseState::InCommandSubstitution, '(') => {
                paren_depth += 1;
                current.push(ch);
                ParseState::InCommandSubstitution
            }
            (ParseState::InCommandSubstitution, ')') => {
                paren_depth -= 1;
                current.push(ch);
                if paren_depth == 0 {
                    ParseState::Normal
                } else {
                    ParseState::InCommandSubstitution
                }
            }
            (ParseState::InCommandSubstitution, _) => {
                current.push(ch);
                ParseState::InCommandSubstitution
            }

            // Whitespace splitting (only in normal state)
            (ParseState::Normal, ' ') => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
                ParseState::Normal
            }

            // Default: append character
            (_, ch) => {
                current.push(ch);
                state
            }
        };
    }

    if !current.is_empty() {
        result.push(current);
    }

    if result.is_empty() {
        vec![input.to_string()]
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_split() {
        assert_eq!(word_split("cmd arg1 arg2"), vec!["cmd", "arg1", "arg2"]);
    }

    #[test]
    fn test_quoted_args() {
        assert_eq!(
            word_split(r#"cmd "arg with spaces""#),
            vec!["cmd", "arg with spaces"]
        );
    }

    #[test]
    fn test_command_substitution() {
        assert_eq!(
            word_split("echo $(get-value)"),
            vec!["echo", "$(get-value)"]
        );
    }

    #[test]
    fn test_command_substitution_with_spaces() {
        assert_eq!(
            word_split("echo $(my-command with some args)"),
            vec!["echo", "$(my-command with some args)"]
        );
    }

    #[test]
    fn test_nested_parens() {
        assert_eq!(
            word_split("echo $(outer (inner))"),
            vec!["echo", "$(outer (inner))"]
        );
    }

    #[test]
    fn test_escaped_dollar() {
        assert_eq!(
            word_split(r"echo \$notacommand"),
            vec!["echo", "$notacommand"]
        );
    }

    #[test]
    fn test_multiple_command_substitutions() {
        assert_eq!(
            word_split("cmd $(first) $(second arg)"),
            vec!["cmd", "$(first)", "$(second arg)"]
        );
    }

    #[test]
    fn test_command_substitution_in_quotes() {
        assert_eq!(
            word_split(r#"echo "value: $(get-val)""#),
            vec!["echo", "value: $(get-val)"]
        );
    }

    #[test]
    fn test_single_quotes_literal() {
        // Single quotes preserve everything literally, including backslashes
        assert_eq!(word_split(r"echo '\n'"), vec!["echo", r"\n"]);
    }

    #[test]
    fn test_single_quotes_no_escape() {
        assert_eq!(word_split(r"echo 'a\tb\nc'"), vec!["echo", r"a\tb\nc"]);
    }

    #[test]
    fn test_single_quotes_with_spaces() {
        assert_eq!(
            word_split("echo 'some random single text'"),
            vec!["echo", "some random single text"]
        );
    }
}
