//! The indexed gap labels of a goal-backward failure message, as the ids of
//! the outcomes a host completion records for them.

use std::collections::BTreeSet;

/// The text before the stage id in the failure message
/// `verify::completion` renders (`Goal-backward verification failed for stage
/// '<id>': <gap>; <gap>`); keep it in step with that message.
const FAILURE_PREFIX: &str = "Goal-backward verification failed for stage '";

/// The `[artifact <n>]`, `[wiring <n>]` and `[wiring_tests <n>]` labels that
/// start a gap of the goal-backward failure in `message`, as `artifact-<n>`,
/// `wiring-<n>` and `wiring_tests-<n>`: sorted and without duplicates. A gap
/// starts the gap list or follows the `; ` separator, so a bracketed label
/// inside a gap's own text (a pattern, a path, test output the plan authored)
/// is not a gap. Any other bracketed text (`[criterion 0]`) is ignored too,
/// and a message that is not a goal-backward failure has none.
pub(crate) fn goal_gap_ids(message: &str) -> Vec<String> {
    let Some(gaps) = message
        .split_once(FAILURE_PREFIX)
        .and_then(|(_, rest)| rest.split_once("': "))
        .map(|(_, gaps)| gaps)
    else {
        return Vec::new();
    };
    let ids: BTreeSet<String> = gaps.split("; ").filter_map(leading_label).collect();
    ids.into_iter().collect()
}

/// The id of the indexed label `gap` starts with, if it names a known list.
fn leading_label(gap: &str) -> Option<String> {
    let (label, index) = gap.strip_prefix('[')?.split_once(']')?.0.split_once(' ')?;
    let known = matches!(label, "artifact" | "wiring" | "wiring_tests");
    let numbered = !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit());
    (known && numbered).then(|| format!("{label}-{index}"))
}

#[cfg(test)]
mod tests {
    use super::goal_gap_ids;

    #[test]
    fn a_label_inside_a_gaps_own_text_is_not_a_gap() {
        let message = "Goal-backward verification failed for stage 's1': \
                       [wiring_tests 0] test 'x' missing expected pattern '[artifact 1]'";

        assert_eq!(goal_gap_ids(message), vec!["wiring_tests-0"]);
    }

    #[test]
    fn a_label_after_a_context_prefix_and_a_separator_is_a_gap() {
        let message = "completing: Goal-backward verification failed for stage 's1': \
                       dead code; [wiring 3] not found; [wiring 3] again";

        assert_eq!(goal_gap_ids(message), vec!["wiring-3"]);
    }

    #[test]
    fn a_message_that_is_not_a_goal_backward_failure_has_no_gaps() {
        assert!(goal_gap_ids("failed: [artifact 1] missing").is_empty());
    }
}
