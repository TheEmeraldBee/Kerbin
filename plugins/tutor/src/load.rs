use kerbin_core::*;

pub static STEPS: &[&str] = &[
    include_str!("./tutor/1.md"),
    include_str!("./tutor/2.md"),
    include_str!("./tutor/3.md"),
    include_str!("./tutor/4.md"),
    include_str!("./tutor/5.md"),
    include_str!("./tutor/6.md"),
    include_str!("./tutor/7.md"),
];

#[derive(Debug, Clone, PartialEq)]
pub enum BufferExpectation {
    Expect(String),
}

impl BufferExpectation {
    pub fn from_html_comment(text: &str) -> Option<Self> {
        let start_marker = "<!--";
        let end_marker = "-->";

        let start = text.find(start_marker)?;
        let end = text.rfind(end_marker)?;

        if start >= end {
            return None;
        }

        let content = text[start + start_marker.len()..end].trim();

        if let Some(expect_text) = content.strip_prefix("Expect:") {
            let expected = expect_text.trim();
            // Remove quotes if present
            let expected = if expected.starts_with('"') && expected.ends_with('"') {
                expected.strip_prefix('"')?.strip_suffix('"')?
            } else {
                expected
            };
            Some(BufferExpectation::Expect(expected.to_string()))
        } else {
            None
        }
    }
}

#[derive(State, Default, Debug)]
pub struct TutorState {
    step: usize,
    expectations: Vec<BufferExpectation>,
}

pub async fn open_default_buffer(bufs: ResMut<Buffers>, log: Res<LogSender>) {
    get!(mut bufs, log);

    let text = include_str!("./tutor/0.md");
    let (expectations, _) = parse_tutor_text(text);

    let mut buffer = TextBuffer::scratch();

    buffer.start_change_group();

    buffer.action(Insert {
        byte: 0,
        content: text.to_string(),
    });

    buffer.commit_change_group();

    buffer.drop_other_cursors();
    buffer.primary_cursor_mut().set_sel(0..=0);

    buffer.undo_stack.clear();
    buffer.redo_stack.clear();

    buffer.path = "<tutor>".to_string();
    buffer.ext = "md".to_string();

    let state = TutorState {
        step: 0,
        expectations,
    };

    buffer.set_state(state);

    log.critical(
        "tutor",
        "Welcome to tutor! This is a tutor that will step you through using your default config, as well as helping you to remove me!",
    );

    bufs.push_new(buffer).await;
    bufs.close_buffer(0).await;
}

pub async fn update_buffer(bufs: ResMut<Buffers>, log: Res<LogSender>) {
    get!(mut bufs, log);

    let mut buf = bufs.cur_buffer_mut().await;

    // Only check if there were user changes
    if buf.byte_changes.is_empty() {
        return;
    }

    // Get all bracket contents in order
    let content = buf.to_string();
    let bracket_contents = extract_brackets(&content);

    let should_load_next = {
        let Some(tutor_state) = buf.get_state::<TutorState>().await else {
            return;
        };

        // Check if expectations match bracket contents
        let all_met = tutor_state.expectations.len() == bracket_contents.len()
            && tutor_state
                .expectations
                .iter()
                .zip(bracket_contents.iter())
                .all(|(exp, bracket_text)| match exp {
                    BufferExpectation::Expect(expected) => expected == bracket_text,
                });

        all_met && !tutor_state.expectations.is_empty()
    };

    if should_load_next {
        log.low("tutor", "Step completed! Loading next step...");
        let finished = load_next_step(&mut buf).await;

        if finished {
            log.critical("tutor", "Tutorial completed! Congratulations!");
        }
    }
}

pub fn parse_tutor_text(text: &str) -> (Vec<BufferExpectation>, String) {
    let mut expectations = Vec::new();

    for line in text.lines() {
        if let Some(expectation) = BufferExpectation::from_html_comment(line) {
            expectations.push(expectation);
        }
    }

    // Keep all text including comments
    (expectations, text.to_string())
}

pub async fn load_next_step(buffer: &mut TextBuffer) -> bool {
    let Some(mut tutor_state) = buffer.get_state_mut::<TutorState>().await else {
        panic!("Called load_next_step on non-tutor file")
    };

    let Some(step_text) = STEPS.get(tutor_state.step) else {
        return true;
    };

    tutor_state.step += 1;

    // Parse the step text for expectations
    let (expectations, clean_text) = parse_tutor_text(step_text);
    tutor_state.expectations = expectations;

    buffer.start_change_group();

    // Clear current buffer content
    let len = buffer.len_bytes();
    if len > 0 {
        buffer.action(Delete { byte: 0, len });
    }

    // Insert the new step content
    buffer.action(Insert {
        byte: 0,
        content: clean_text,
    });

    buffer.commit_change_group();

    buffer.drop_other_cursors();
    buffer.primary_cursor_mut().set_sel(0..=0);

    buffer.undo_stack.clear();
    buffer.redo_stack.clear();

    false
}

fn extract_brackets(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '[' {
            let mut content = String::new();
            for ch in chars.by_ref() {
                if ch == ']' {
                    break;
                }
                content.push(ch);
            }
            results.push(content);
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expectation_comments() {
        assert_eq!(
            BufferExpectation::from_html_comment("<!-- Expect: \"\" -->"),
            Some(BufferExpectation::Expect("".to_string()))
        );

        assert_eq!(
            BufferExpectation::from_html_comment("<!-- Expect: \"hello, world\" -->"),
            Some(BufferExpectation::Expect("hello, world".to_string()))
        );

        assert_eq!(BufferExpectation::from_html_comment("not a comment"), None);
    }

    #[test]
    fn test_extract_brackets() {
        let text = "[Delete] and then [] and [hello, world]";
        let brackets = extract_brackets(text);

        assert_eq!(brackets.len(), 3);
        assert_eq!(brackets[0], "Delete");
        assert_eq!(brackets[1], "");
        assert_eq!(brackets[2], "hello, world");
    }

    #[test]
    fn test_parse_tutor_text() {
        let input = "# Tutorial\n[Delete] <!-- Expect: \"\" -->\nNext line.";
        let (expectations, clean_text) = parse_tutor_text(input);

        assert_eq!(expectations.len(), 1);
        assert_eq!(expectations[0], BufferExpectation::Expect("".to_string()));
        assert_eq!(clean_text, input);
    }
}
