//! A retry of a typed block that fails over a cause no tick can clear hands the
//! stage to the operator; any other failure keeps the block and names itself.

use super::super::resolver_spawn::test_fixtures::git_ok;
use super::tests::{blocked_stage, on_disk};
use crate::models::stage::StageStatus;

const ID: &str = "s";

#[test]
fn a_deleted_stage_branch_routes_the_typed_block_to_review() {
    let (repo, mut orchestrator) = blocked_stage();
    git_ok(repo.path(), &["update-ref", "-d", "refs/heads/loom/s"]);

    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    let held = on_disk(&orchestrator);
    assert_eq!(held.status, StageStatus::NeedsHumanReview);
    assert!(held.merge.block.is_none(), "the stale remedy goes");
    let reason = held.review_reason.expect("the operator is told why");
    assert!(reason.contains("loom/s is missing"), "{reason}");
}

#[test]
fn a_branch_that_moved_after_verification_routes_the_typed_block_to_review() {
    let (repo, mut orchestrator) = blocked_stage();
    let worktree = repo.path().join(".worktrees").join(ID);
    git_ok(
        &worktree,
        &["commit", "-q", "--allow-empty", "-m", "after verification"],
    );

    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));

    let held = on_disk(&orchestrator);
    assert_eq!(held.status, StageStatus::NeedsHumanReview);
    assert!(held.merge.block.is_none());
    let reason = held.review_reason.expect("the operator is told why");
    assert!(reason.contains("moved"), "{reason}");
}

/// Repeated failures with one cause write the stage once: the retry runs on
/// every tick, and the reason it records must not churn the file.
#[test]
fn a_repeated_failure_is_recorded_once() {
    let (repo, mut orchestrator) = blocked_stage();
    git_ok(repo.path(), &["update-ref", "-d", "refs/heads/main"]);

    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    let first = on_disk(&orchestrator);
    orchestrator.retry_blocked_merge(&first);

    let second = on_disk(&orchestrator);
    assert_eq!(second.updated_at, first.updated_at);
    assert_eq!(second.failure_info, first.failure_info);
    assert_eq!(second.status, StageStatus::MergeBlocked);
}
