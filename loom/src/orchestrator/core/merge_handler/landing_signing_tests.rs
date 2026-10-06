//! Signing failures of loom's own merge commit on the merge routes beside the
//! landing: the first auto-merge and the relayed merge hold.

use super::{main_tip, on_disk, set_status, worktree_stage, Session, StageStatus, ID};
use crate::git::signing::tests::fake_signer;
use crate::models::failure::FailureType;

#[test]
fn a_first_auto_merge_signing_failure_routes_to_review() {
    let (repo, mut orchestrator) = worktree_stage();
    fake_signer(repo.path(), true);
    let main_before = main_tip(&repo);

    assert!(!orchestrator.try_auto_merge(ID));

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    assert!(reason.contains("merge commit signing failed"), "{reason}");
    assert!(reason.contains("fake signer refused"), "{reason}");
    assert!(!matches!(
        stage.failure_info,
        Some(info) if info.failure_type == FailureType::InfrastructureError
    ));
    assert_eq!(main_tip(&repo), main_before);
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
}

/// A tracked resolver for the stage in `status`, held for signing.
fn hold_with_resolver(status: StageStatus) {
    let (_repo, mut orchestrator) = worktree_stage();
    set_status(&orchestrator, status);
    let resolver = Session::new_merge("loom/s".to_string(), "main".to_string());
    let resolver_id = resolver.id.clone();
    orchestrator
        .active_sessions
        .insert(ID.to_string(), resolver);

    let outcome = orchestrator.hold_merge_for_signing(ID, "fake signer refused");

    assert!(outcome.contains("held for the operator"), "{outcome}");
    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    assert!(reason.contains(&resolver_id), "{reason}");
    assert!(
        reason.contains("loom stage human-review s --approve"),
        "{reason}"
    );
    assert!(
        !orchestrator.active_sessions.contains_key(ID)
            || reason.contains("could not confirm it stopped"),
        "{reason}"
    );
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
}

#[test]
fn a_relayed_merge_signing_hold_stops_the_resolver() {
    hold_with_resolver(StageStatus::MergeConflict);
    hold_with_resolver(StageStatus::MergeBlocked);
}
