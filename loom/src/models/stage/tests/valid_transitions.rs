use crate::models::stage::StageStatus;

#[test]
fn test_valid_transitions_waiting_for_deps() {
    let transitions = StageStatus::WaitingForDeps.valid_transitions();
    assert_eq!(transitions, vec![StageStatus::Queued, StageStatus::Skipped]);
}

#[test]
fn test_valid_transitions_executing() {
    let transitions = StageStatus::Executing.valid_transitions();
    assert_eq!(transitions.len(), 8);
    assert!(transitions.contains(&StageStatus::Completed));
    assert!(transitions.contains(&StageStatus::Blocked));
    assert!(transitions.contains(&StageStatus::NeedsHandoff));
    assert!(transitions.contains(&StageStatus::WaitingForInput));
    assert!(transitions.contains(&StageStatus::MergeConflict));
    assert!(transitions.contains(&StageStatus::CompletedWithFailures));
    assert!(transitions.contains(&StageStatus::MergeBlocked));
    assert!(transitions.contains(&StageStatus::NeedsHumanReview));
}

#[test]
fn test_valid_transitions_completed() {
    let transitions = StageStatus::Completed.valid_transitions();
    assert!(transitions.is_empty());
}

#[test]
fn test_valid_transitions_includes_skipped() {
    let transitions = StageStatus::WaitingForDeps.valid_transitions();
    assert!(transitions.contains(&StageStatus::Skipped));

    let transitions = StageStatus::Queued.valid_transitions();
    assert!(transitions.contains(&StageStatus::Skipped));

    let transitions = StageStatus::Blocked.valid_transitions();
    assert!(transitions.contains(&StageStatus::Skipped));

    let transitions = StageStatus::Executing.valid_transitions();
    assert!(!transitions.contains(&StageStatus::Skipped));
}

#[test]
fn test_valid_transitions_queued_includes_blocked() {
    // Queued stages can be blocked due to pre-execution failures
    let transitions = StageStatus::Queued.valid_transitions();
    assert!(transitions.contains(&StageStatus::Blocked));
    assert!(transitions.contains(&StageStatus::Executing));
    assert!(transitions.contains(&StageStatus::Skipped));
    assert_eq!(transitions.len(), 3);
}

#[test]
fn test_valid_transitions_executing_includes_merge_conflict() {
    let transitions = StageStatus::Executing.valid_transitions();
    assert!(transitions.contains(&StageStatus::MergeConflict));
    // Completed, Blocked, NeedsHandoff, WaitingForInput, MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview
    assert_eq!(transitions.len(), 8);
}

#[test]
fn test_valid_transitions_merge_conflict() {
    let transitions = StageStatus::MergeConflict.valid_transitions();
    assert!(transitions.contains(&StageStatus::Completed));
    assert!(transitions.contains(&StageStatus::Blocked));
    assert_eq!(transitions.len(), 2);
}

#[test]
fn test_valid_transitions_executing_includes_completed_with_failures() {
    let transitions = StageStatus::Executing.valid_transitions();
    assert!(transitions.contains(&StageStatus::CompletedWithFailures));
}

#[test]
fn test_valid_transitions_completed_with_failures() {
    let transitions = StageStatus::CompletedWithFailures.valid_transitions();
    assert!(transitions.contains(&StageStatus::Queued));
    assert!(transitions.contains(&StageStatus::Executing));
    assert!(transitions.contains(&StageStatus::Completed)); // For re-verify
    assert_eq!(transitions.len(), 3);
}

#[test]
fn test_valid_transitions_executing_includes_merge_blocked() {
    let transitions = StageStatus::Executing.valid_transitions();
    assert!(transitions.contains(&StageStatus::MergeBlocked));
}

#[test]
fn test_valid_transitions_merge_blocked() {
    let transitions = StageStatus::MergeBlocked.valid_transitions();
    assert!(transitions.contains(&StageStatus::Queued));
    assert!(transitions.contains(&StageStatus::Executing));
    assert_eq!(transitions.len(), 2);
}

#[test]
fn test_valid_transitions_executing_count() {
    let transitions = StageStatus::Executing.valid_transitions();
    // Completed, Blocked, NeedsHandoff, WaitingForInput, MergeConflict, CompletedWithFailures, MergeBlocked, NeedsHumanReview
    assert_eq!(transitions.len(), 8);
}

#[test]
fn test_valid_transitions_executing_includes_needs_human_review() {
    let transitions = StageStatus::Executing.valid_transitions();
    assert!(transitions.contains(&StageStatus::NeedsHumanReview));
}

#[test]
fn test_valid_transitions_needs_human_review() {
    let transitions = StageStatus::NeedsHumanReview.valid_transitions();
    assert!(transitions.contains(&StageStatus::Executing));
    assert!(transitions.contains(&StageStatus::Completed));
    assert!(transitions.contains(&StageStatus::Blocked));
    assert_eq!(transitions.len(), 3);
}
