use thiserror::Error;

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A bare word or de-quoted string.
    Word(String),
    /// Raw inner content of `$(...)`.
    CommandSubst(String),
    /// Template name after `%`.
    Variable(String),
    /// Items inside `[...]`.
    List(Vec<Token>),
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum LexError {
    #[error("Unclosed string literal")]
    UnclosedString,
    #[error("Unclosed command substitution")]
    UnclosedCommandSubst,
    #[error("Unclosed list")]
    UnclosedList,
}

/// Process a backslash escape sequence, returning the resolved character.
///
/// - `\n` → newline, `\t` → tab, `\r` → carriage return, `\0` → null
/// - `\\`, `\'`, `\"` → the literal character
/// - Any other `\X` → `X` (unknown escape passes through)
fn unescape(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        other => other,
    }
}

/// Tokenize an input string into a `Vec<Token>`.
///
/// Handles (in order of priority):
/// - `\X` escape: processed as an escape sequence (`\n`→newline, `\t`→tab, etc.); not active inside single quotes
/// - `'...'` single quotes: everything literal until closing `'` (no escape processing)
/// - `"..."` double quotes: preserves spaces, `\X` escape sequences active inside
/// - `$(...)` command substitution → `Token::CommandSubst(inner)`
/// - `%%` → literal `%`; `%name` → `Token::Variable(name)`
/// - `[...]` list → `Token::List(inner_tokens)`
/// - Whitespace splits tokens
/// - Everything else → `Token::Word`
///
/// Returns an empty vec for empty input.
pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_current = false;
    let mut chars = input.chars().peekable();

    macro_rules! flush_word {
        () => {
            if has_current {
                tokens.push(Token::Word(std::mem::take(&mut current)));
                has_current = false;
            }
        };
    }

    while let Some(ch) = chars.next() {
        match ch {
            // Escape sequence: \X → processed character
            '\\' => {
                let next = chars.next().unwrap_or('\\');
                current.push(unescape(next));
                has_current = true;
            }
            // Single quotes: everything literal until closing ' (no escape processing)
            '\'' => {
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => current.push(c),
                        None => return Err(LexError::UnclosedString),
                    }
                }
                has_current = true;
            }
            // Double quotes: preserves spaces, \X escape sequences active inside.
            // $() and % are kept as literal text (not tokenized separately).
            '"' => {
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => {
                            let next = chars.next().unwrap_or('\\');
                            current.push(unescape(next));
                        }
                        Some(c) => current.push(c),
                        None => return Err(LexError::UnclosedString),
                    }
                }
                has_current = true;
            }
            // Command substitution: $( ... )
            '$' if chars.peek() == Some(&'(') => {
                chars.next(); // consume '('
                flush_word!();
                let inner = consume_until_close_paren(&mut chars)?;
                tokens.push(Token::CommandSubst(inner));
            }
            // Variable: %% → literal %, %name → Token::Variable
            '%' => {
                if chars.peek() == Some(&'%') {
                    // %% is an escaped literal %
                    chars.next();
                    current.push('%');
                    has_current = true;
                } else {
                    let mut name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            name.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if name.is_empty() {
                        current.push('%');
                        has_current = true;
                    } else {
                        flush_word!();
                        tokens.push(Token::Variable(name));
                    }
                }
            }
            // List: [ ... ]
            '[' => {
                flush_word!();
                let inner = consume_until_close_bracket(&mut chars)?;
                let inner_tokens = tokenize(&inner)?;
                tokens.push(Token::List(inner_tokens));
            }
            // Whitespace: splits tokens (in normal mode only)
            ' ' | '\t' | '\n' => {
                flush_word!();
            }
            // Regular character
            _ => {
                current.push(ch);
                has_current = true;
            }
        }
    }

    flush_word!();

    Ok(tokens)
}

fn consume_until_close_paren(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<String, LexError> {
    let mut inner = String::new();
    let mut depth = 1;

    for ch in chars.by_ref() {
        match ch {
            '(' => {
                depth += 1;
                inner.push(ch);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(inner);
                }
                inner.push(ch);
            }
            _ => inner.push(ch),
        }
    }

    Err(LexError::UnclosedCommandSubst)
}

fn consume_until_close_bracket(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<String, LexError> {
    let mut inner = String::new();
    let mut depth = 1;

    for ch in chars.by_ref() {
        match ch {
            '[' => {
                depth += 1;
                inner.push(ch);
            }
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(inner);
                }
                inner.push(ch);
            }
            _ => inner.push(ch),
        }
    }

    Err(LexError::UnclosedList)
}

/// Flatten a token list back into a single whitespace-separated string.
/// Used by `expand_str` as a compatibility shim.
pub fn flatten_tokens(tokens: Vec<Token>) -> String {
    tokens
        .into_iter()
        .map(|t| match t {
            Token::Word(s) => s,
            Token::CommandSubst(s) => format!("$({})", s),
            Token::Variable(s) => format!("%{}", s),
            Token::List(items) => format!("[{}]", flatten_tokens(items)),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_words() {
        assert_eq!(
            tokenize("cmd arg1 arg2").unwrap(),
            vec![
                Token::Word("cmd".to_string()),
                Token::Word("arg1".to_string()),
                Token::Word("arg2".to_string()),
            ]
        );
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(tokenize("").unwrap(), vec![]);
    }

    #[test]
    fn test_list_simple() {
        assert_eq!(
            tokenize("[a b c]").unwrap(),
            vec![Token::List(vec![
                Token::Word("a".to_string()),
                Token::Word("b".to_string()),
                Token::Word("c".to_string()),
            ])]
        );
    }

    #[test]
    fn test_list_mixed() {
        let tokens = tokenize("cmd [a b] other").unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(&tokens[0], Token::Word(s) if s == "cmd"));
        assert!(matches!(&tokens[1], Token::List(_)));
        assert!(matches!(&tokens[2], Token::Word(s) if s == "other"));
    }

    #[test]
    fn test_nested_list() {
        let tokens = tokenize("[[a b] c]").unwrap();
        assert_eq!(tokens.len(), 1);
        if let Token::List(items) = &tokens[0] {
            assert_eq!(items.len(), 2);
            assert!(matches!(&items[0], Token::List(_)));
            assert!(matches!(&items[1], Token::Word(s) if s == "c"));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_quoted_items_in_list() {
        let tokens = tokenize(r#"["a b" c]"#).unwrap();
        if let Token::List(items) = &tokens[0] {
            assert_eq!(items.len(), 2);
            assert!(matches!(&items[0], Token::Word(s) if s == "a b"));
            assert!(matches!(&items[1], Token::Word(s) if s == "c"));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_command_subst() {
        let tokens = tokenize("echo $(get-value)").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::CommandSubst(s) if s == "get-value"));
    }

    #[test]
    fn test_nested_command_subst() {
        let tokens = tokenize("echo $(outer (inner))").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(
            matches!(&tokens[1], Token::CommandSubst(s) if s == "outer (inner)")
        );
    }

    #[test]
    fn test_variable() {
        let tokens = tokenize("echo %my_var").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Variable(s) if s == "my_var"));
    }

    #[test]
    fn test_escape_newline() {
        // \n outside quotes → actual newline character
        let tokens = tokenize(r"echo \n").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == "\n"));
    }

    #[test]
    fn test_escape_tab() {
        let tokens = tokenize(r"echo \t").unwrap();
        assert!(matches!(&tokens[1], Token::Word(s) if s == "\t"));
    }

    #[test]
    fn test_escape_backslash() {
        let tokens = tokenize(r"echo \\").unwrap();
        assert!(matches!(&tokens[1], Token::Word(s) if s == "\\"));
    }

    #[test]
    fn test_escape_dollar() {
        // \$ → literal $ (no command subst)
        let tokens = tokenize(r"echo \$notacommand").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == "$notacommand"));
    }

    #[test]
    fn test_single_quotes_literal() {
        // Single quotes: everything literal, no escape processing
        let tokens = tokenize(r"echo '\n'").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == r"\n"));
    }

    #[test]
    fn test_double_quotes_space() {
        let tokens = tokenize(r#"cmd "arg with spaces""#).unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == "arg with spaces"));
    }

    #[test]
    fn test_double_quotes_escape_newline() {
        // \n inside double quotes → actual newline
        let tokens = tokenize("cmd \"line1\\nline2\"").unwrap();
        assert!(matches!(&tokens[1], Token::Word(s) if s == "line1\nline2"));
    }

    #[test]
    fn test_percent_percent_literal() {
        // %% → single literal %
        let tokens = tokenize("echo %%").unwrap();
        assert!(matches!(&tokens[1], Token::Word(s) if s == "%"));
    }

    #[test]
    fn test_percent_percent_with_suffix() {
        // %%hello_world → literal %hello_world (not a variable)
        let tokens = tokenize("echo %%hello_world").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == "%hello_world"));
    }

    #[test]
    fn test_escape_command_subst() {
        // \$(...) → literal word "$(cmd)", NOT a CommandSubst token
        // \$ → $, then (cmd) continues in the same bare word
        let tokens = tokenize(r"echo \$(cmd)").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == "$(cmd)"));
        assert!(tokens.iter().all(|t| !matches!(t, Token::CommandSubst(_))));
    }

    #[test]
    fn test_single_quotes_prevent_shell() {
        // Single quotes prevent $() from being treated as command substitution
        let tokens = tokenize("echo '$(cmd)'").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == "$(cmd)"));
        assert!(tokens.iter().all(|t| !matches!(t, Token::CommandSubst(_))));
    }

    #[test]
    fn test_single_quotes_prevent_variable() {
        // Single quotes prevent %name from being treated as a variable
        let tokens = tokenize("echo '%my_var'").unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[1], Token::Word(s) if s == "%my_var"));
        assert!(tokens.iter().all(|t| !matches!(t, Token::Variable(_))));
    }

    #[test]
    fn test_mixed_cmd_list() {
        let tokens = tokenize("cmd $(get-list) [a b]").unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(&tokens[0], Token::Word(s) if s == "cmd"));
        assert!(matches!(&tokens[1], Token::CommandSubst(s) if s == "get-list"));
        assert!(matches!(&tokens[2], Token::List(_)));
    }
}
