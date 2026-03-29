use crate::*;

pub async fn auto_pairs_intercept(cmd: &BufferCommand, state: &mut State) -> InterceptorResult {
    let typed_char = match cmd {
        BufferCommand::Append { text, .. } if text.chars().count() == 1 => {
            text.chars().next().unwrap()
        }
        _ => return InterceptorResult::Allow,
    };

    let char_at_cursor = {
        let buffers = state.lock_state::<Buffers>().await;
        buffers.cur_buffer_as::<TextBuffer>().await.and_then(|tb| {
            let byte = tb.primary_cursor().get_cursor_byte();
            tb.char(tb.byte_to_char_clamped(byte))
        })
    };

    let auto_pairs = state.lock_state::<AutoPairs>().await;

    if let Some(pair) = auto_pairs.find_by_open(typed_char) {
        let closer = pair.close.to_string();
        let is_symmetric = pair.open == pair.close;
        drop(auto_pairs);

        if is_symmetric && char_at_cursor == Some(typed_char) {
            return InterceptorResult::Replace(vec![Box::new(BufferCommand::MoveChars {
                chars: 1,
                extend: false,
            })]);
        }

        return InterceptorResult::After(vec![Box::new(BufferCommand::Insert(closer))]);
    }

    if auto_pairs.find_by_close(typed_char).is_some() {
        drop(auto_pairs);
        if char_at_cursor == Some(typed_char) {
            return InterceptorResult::Replace(vec![Box::new(BufferCommand::MoveChars {
                chars: 1,
                extend: false,
            })]);
        }
    }

    InterceptorResult::Allow
}
