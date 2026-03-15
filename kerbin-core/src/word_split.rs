use kerbin_input::{Token, tokenize};

/// Split an input string into a flat list of words, preserving `$(...)` and `%name` as opaque
/// strings. Lists (`[...]`) are dropped since they cannot be represented as flat strings.
///
/// This is a thin compatibility wrapper around [`tokenize`]. New code should use `tokenize`
/// directly to take advantage of `Token::List` and proper `Token::CommandSubst`/`Token::Variable`
/// handling.
pub fn word_split(input: &str) -> Vec<String> {
    tokenize(input)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| match t {
            Token::Word(s) => Some(s),
            Token::CommandSubst(s) => Some(format!("$({})", s)),
            Token::Variable(s) => Some(format!("%{}", s)),
            Token::List(_) => None,
        })
        .collect()
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
