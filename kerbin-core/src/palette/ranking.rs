#[inline]
/// Ranks a piece of text based on a search term.
pub fn rank(ranker: &str, text: &str) -> Option<i32> {
    // Check to make sure that neither is empty

    // If the search term is none, all text should have the same score.
    if ranker.is_empty() {
        return Some(0);
    }

    // We can't score a text that doesn't exist.
    if text.is_empty() {
        return None;
    }

    // Set up variables for scoring
    let mut score = 0;

    let mut distance = 0;
    let mut i = 0;

    // Turn terms into character lists. Also, make them lowercase to ignore case.
    let ranker = ranker.to_lowercase().chars().collect::<Vec<char>>();
    let text = text.to_lowercase().chars().collect::<Vec<char>>();

    for chr in text {
        // If we've found the character, increment the score by how far apart the letters were.
        if chr == ranker[i] {
            score += distance;
            i += 1;
            distance = 0;

            // If we have passed the end, there is nothing more to score.
            if i >= ranker.len() {
                break;
            }
        }
        // Otherwise, increase the distance by one.
        else {
            distance += 1;
        }
    }

    // If the full string isn't there, return no score.
    if i != ranker.len() {
        return None;
    }

    // Otherwise return the score
    Some(score)
}
