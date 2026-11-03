//! Restatement measure of the seal: an overview sentence that repeats the member sentence it
//! cites says nothing the member does not, and opening it would show the same text twice.

use std::collections::BTreeSet;

/// Dice similarity at or above which an overview sentence restates a member sentence.
const RESTATE_THRESHOLD: f64 = 0.75;

/// True when `overview_text` and `member_text` (both without links) share enough words to be
/// the same statement. Similarity is `2|A∩B| / (|A|+|B|)` over the lowercase ASCII
/// letter-and-digit word sets of the two texts; an empty text never restates anything.
pub(super) fn restates(overview_text: &str, member_text: &str) -> bool {
    let (a, b) = (words(overview_text), words(member_text));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let shared = a.intersection(&b).count();
    2.0 * shared as f64 / (a.len() + b.len()) as f64 >= RESTATE_THRESHOLD
}

/// The word set of `text`: runs of ASCII letters and digits, lowercased.
fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMBER: &str = "File mode is the planning app's reader and editor, adding server \
                          document, tree, save, diff and search routes plus in-process plan \
                          validation.";

    #[test]
    fn identical_text_restates() {
        assert!(restates("Alpha one holds.", "Alpha one holds."));
    }

    #[test]
    fn case_and_punctuation_do_not_hide_a_restatement() {
        assert!(restates("ALPHA, one: holds!", "alpha one holds"));
    }

    #[test]
    fn a_close_paraphrase_restates() {
        let overview = "File mode adds the server's document, tree, save, diff and search \
                        routes, with in-process plan validation.";
        assert!(restates(overview, MEMBER));
    }

    #[test]
    fn a_compression_of_two_members_restates_neither() {
        let first = "The reader opens every plan document in a single column with its outline.";
        let second =
            "The editor saves a changed document and shows its diff against the stored copy.";
        let overview = "Plans are read in one column and edited with saved diffs.";
        assert!(!restates(overview, first));
        assert!(!restates(overview, second));
    }

    #[test]
    fn unrelated_sentences_do_not_restate() {
        assert!(!restates("Alpha one holds.", "Beta two fails."));
    }

    #[test]
    fn empty_text_restates_nothing() {
        assert!(!restates("", ""));
        assert!(!restates("", "Alpha one holds."));
        assert!(!restates("Alpha one holds.", "  ... "));
    }
}
