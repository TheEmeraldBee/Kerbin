#[inline]
/// Fuzzy-ranks `text` against `ranker`. Returns `Some(score)` if all chars of
/// `ranker` appear in order in `text` (lower is better), or `None` if they don't.
pub fn rank(ranker: &str, text: &str) -> Option<i32> {
    if ranker.is_empty() {
        return Some(0);
    }
    if text.is_empty() {
        return None;
    }

    let mut score = 0;
    let mut distance = 0;
    let mut i = 0;

    let ranker = ranker.to_lowercase().chars().collect::<Vec<char>>();
    let text = text.to_lowercase().chars().collect::<Vec<char>>();

    for chr in text {
        if chr == ranker[i] {
            score += distance;
            i += 1;
            distance = 0;
            if i >= ranker.len() {
                break;
            }
        } else {
            distance += 1;
        }
    }

    if i != ranker.len() {
        return None;
    }

    Some(score)
}
