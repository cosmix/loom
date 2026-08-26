//! Tests for the `cleanup_warning` field `MergeLifecycle::cleanup` records on
//! the stage when a deferred cleanup is refused or fails, and clears when a
//! later cleanup succeeds. Split out of `tests.rs` to keep that file under the
//! maintainability line limit; reuses its repo/stage fixtures.

use std::fs;

use super::tests::{
    cleanup_from_outside, merge_stage_branch, repo_with_stage_commit, worktree_of,
    write_stage_record,
};
use super::CleanupOutcome;
use crate::verify::transitions::{load_stage, update_stage};

#[test]
fn a_refused_cleanup_is_recorded_on_the_stage() {
    let stage_id = "unmerged-recorded-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let work_dir = root.join(".work");
    write_stage_record(&work_dir, stage_id, &head);

    let outcome = cleanup_from_outside(root, stage_id);
    assert!(
        matches!(outcome, CleanupOutcome::Refused { .. }),
        "got {outcome:?}"
    );

    let warning = load_stage(stage_id, &work_dir)
        .unwrap()
        .cleanup_warning
        .expect("a refused cleanup must record a warning");
    assert!(
        warning.starts_with("refused:"),
        "warning must be tagged as a refusal: {warning}"
    );
}

#[test]
fn a_successful_cleanup_clears_the_recorded_warning() {
    let stage_id = "merged-recorded-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let work_dir = root.join(".work");
    write_stage_record(&work_dir, stage_id, &head);
    update_stage(stage_id, &work_dir, |s| {
        s.cleanup_warning = Some("failed: earlier".to_string());
        Ok(())
    })
    .unwrap();
    merge_stage_branch(root, stage_id);

    let outcome = cleanup_from_outside(root, stage_id);
    assert!(
        matches!(outcome, CleanupOutcome::Done(_)),
        "got {outcome:?}"
    );

    let stage = load_stage(stage_id, &work_dir).unwrap();
    assert_eq!(
        stage.cleanup_warning, None,
        "a successful cleanup must clear the recorded warning"
    );
}

#[test]
fn a_failed_removal_is_recorded_on_the_stage() {
    let stage_id = "unremovable-recorded-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let work_dir = root.join(".work");
    write_stage_record(&work_dir, stage_id, &head);
    merge_stage_branch(root, stage_id);
    let claude_dir = worktree_of(root, stage_id).join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("rogue.txt"), "not scaffold\n").unwrap();

    let outcome = cleanup_from_outside(root, stage_id);
    assert!(
        matches!(outcome, CleanupOutcome::Failed(_)),
        "got {outcome:?}"
    );

    let warning = load_stage(stage_id, &work_dir)
        .unwrap()
        .cleanup_warning
        .expect("a failed cleanup must record a warning");
    assert!(
        warning.starts_with("failed:"),
        "warning must be tagged as a failure: {warning}"
    );
}
