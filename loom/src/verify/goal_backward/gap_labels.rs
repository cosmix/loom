//! The indexed gap labels of a goal-backward failure message, as the ids of
//! the outcomes a host completion records for them.

use std::collections::BTreeSet;

/// Every `[artifact <n>]`, `[wiring <n>]` and `[wiring_tests <n>]` label in
/// `message`, as `artifact-<n>`, `wiring-<n>` and `wiring_tests-<n>`: sorted
/// and without duplicates. Any other bracketed text (`[criterion 0]`, the
/// stage id) is not a goal-backward gap and is ignored.
pub(crate) fn goal_gap_ids(message: &str) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for (open, _) in message.match_indices('[') {
        let rest = &message[open + 1..];
        let Some(end) = rest.find(']') else { continue };
        if let Some((label, index)) = rest[..end].split_once(' ') {
            let known = matches!(label, "artifact" | "wiring" | "wiring_tests");
            if known && !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) {
                ids.insert(format!("{label}-{index}"));
            }
        }
    }
    ids.into_iter().collect()
}
