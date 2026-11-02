//! A live session for the stage defers cleanup, whatever the caller's cwd.

use super::test_support::{finish_session, write_live_session};
use super::tests::{
    cleanup_from_outside, merge_stage_branch, repo_with_stage_commit, worktree_of,
    write_stage_record,
};
use super::CleanupOutcome;
use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::branch_exists_strict;
use crate::models::session::SessionType;
use crate::verify::transitions::load_stage;

#[test]
fn a_live_session_defers_cleanup_and_keeps_the_worktree_and_branch() {
    let stage_id = "live-session-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    write_stage_record(&work_dir, stage_id, &head);
    merge_stage_branch(root, stage_id);
    let mut session = write_live_session(&work_dir, stage_id, SessionType::Merge);

    let outcome = cleanup_from_outside(root, stage_id);

    match outcome {
        CleanupOutcome::Deferred { reason } => {
            assert!(reason.contains(&session.id), "{reason}");
            assert!(reason.contains("merge"), "{reason}");
        }
        other => panic!("expected a deferral, got {other:?}"),
    }
    assert!(worktree_of(root, stage_id).exists());
    assert!(branch_exists_strict(&branch_name_for_stage(stage_id), root).unwrap());
    assert!(
        load_stage(stage_id, &work_dir)
            .unwrap()
            .cleanup_warning
            .is_none(),
        "a deferral is not a warning"
    );

    finish_session(&work_dir, &mut session);
    let outcome = cleanup_from_outside(root, stage_id);
    assert!(
        matches!(outcome, CleanupOutcome::Done(_)),
        "got {outcome:?}"
    );
    assert!(!worktree_of(root, stage_id).exists());
}

#[test]
fn a_live_session_of_another_stage_does_not_defer_cleanup() {
    let stage_id = "idle-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    write_stage_record(&work_dir, stage_id, &head);
    merge_stage_branch(root, stage_id);
    write_live_session(&work_dir, "some-other-stage", SessionType::Stage);

    let outcome = cleanup_from_outside(root, stage_id);

    assert!(
        matches!(outcome, CleanupOutcome::Done(_)),
        "got {outcome:?}"
    );
}

#[test]
fn malformed_session_inventory_defers_cleanup() {
    let stage_id = "merged-stage";
    let (temp, head) = repo_with_stage_commit(stage_id);
    let root = temp.path();
    let work = root.join(".loom").join("work");
    write_stage_record(&work, stage_id, &head);
    merge_stage_branch(root, stage_id);
    let sessions = work.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("unknown-writer.md"), "invalid record").unwrap();

    let outcome = cleanup_from_outside(root, stage_id);

    assert!(matches!(outcome, CleanupOutcome::Deferred { .. }));
    assert!(worktree_of(root, stage_id).exists());
    assert!(branch_exists_strict(&branch_name_for_stage(stage_id), root).unwrap());
}
