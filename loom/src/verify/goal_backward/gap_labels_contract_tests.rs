//! Contract `goal-gap-labels-become-outcomes`: the indexed goal-backward gap
//! labels in a failure message become sorted, deduplicated outcome ids.
use crate::verify::goal_backward::gap_labels::goal_gap_ids;

#[test]
fn goal_gap_labels_become_outcome_ids() {
    let message = "Goal-backward verification failed for stage 's1': [artifact 1] No files match \
                   artifact pattern: x.md; [wiring 0] pattern not found; [artifact 1] stub found; \
                   [criterion 3] ignored; [wiring_tests 2] missing";
    assert_eq!(
        goal_gap_ids(message),
        vec!["artifact-1", "wiring-0", "wiring_tests-2"]
    );
    assert!(
        goal_gap_ids("Goal-backward verification failed for stage 's1': dead code found")
            .is_empty(),
        "no label, no id"
    );
}
