//! `merge-resolved` when the resolved content fails completion verification:
//! the stage goes back to a stage session once a merge commit is proved, and is
//! refused otherwise.

use crate::fs::inbox::LedgerOutcome;
use crate::models::session::SessionStatus;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, update_stage};

use super::test_support::{fixture, Fixture, STAGE};
use super::tests_merge::{
    assert_refused, conflict_stage, git, orchestrator, repository, resolve, resolved_stage,
    worktree_of,
};

#[test]
fn merge_resolved_hands_failing_completion_back_without_replacing_original_proof() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.acceptance = vec![crate::models::stage::AcceptanceCriterion::Simple(
            "false".into(),
        )];
        Ok(())
    })
    .unwrap();
    let original = load_stage(STAGE, &fx.work_dir).unwrap().completed_commit;
    let main = git(&fx.repo_root, &["rev-parse", "main"]);
    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit, original);
    assert_eq!(git(&fx.repo_root, &["rev-parse", "main"]), main);
}

/// Make the acceptance criteria of the fixture stage fail.
fn fail_acceptance(fx: &Fixture) {
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.acceptance = vec![crate::models::stage::AcceptanceCriterion::Simple(
            "false".into(),
        )];
        Ok(())
    })
    .unwrap();
}

#[test]
fn merge_resolved_with_failing_verification_and_no_merge_commit_is_refused() {
    let fx = fixture();
    repository(&fx);
    let own_commit = git(&worktree_of(&fx), &["rev-parse", "HEAD"]);
    conflict_stage(&fx, Some(&own_commit));
    fail_acceptance(&fx);
    let mut orchestrator = orchestrator(&fx);

    assert_refused(&fx, &mut orchestrator);

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.merge.handbacks, 0);
    assert_eq!(stage.merge.handback_refusal, None);
}

#[test]
fn merge_resolved_with_a_typed_block_and_failing_verification_is_refused() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    fail_acceptance(&fx);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.status = StageStatus::MergeBlocked;
        stage.merge.block = Some(crate::git::MergeBlock::TargetMoved);
        Ok(())
    })
    .unwrap();

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Refused)
    );

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    assert_eq!(stage.merge.block, Some(crate::git::MergeBlock::TargetMoved));
    assert_eq!(stage.merge.handback_refusal, None);
}

#[test]
fn merge_resolved_with_failing_verification_stops_a_live_tracked_resolver() {
    let fx = fixture();
    let mut orchestrator = resolved_stage(&fx);
    fail_acceptance(&fx);
    let mut tracked =
        crate::models::session::Session::new_merge(format!("loom/{STAGE}"), "main".to_string());
    tracked.assign_to_stage(STAGE.to_string());
    tracked.status = SessionStatus::Running;
    crate::fs::session_files::save_session(&tracked, &fx.work_dir).unwrap();
    // A start time that never matches reads as a process proven gone once killed.
    let pids = fx.work_dir.join("pids");
    std::fs::create_dir_all(&pids).unwrap();
    std::fs::write(
        pids.join(format!("{}-{}.pid", tracked.tracking_key, tracked.id)),
        format!("{}\n{}\n", std::process::id(), u64::MAX),
    )
    .unwrap();
    orchestrator
        .active_sessions
        .insert(STAGE.to_string(), tracked.clone());

    assert_eq!(
        resolve(&fx, &mut orchestrator),
        Some(LedgerOutcome::Applied)
    );

    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    let record = crate::fs::session_files::load_session_exact(&fx.work_dir, &tracked.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        record.exit_reason,
        Some(crate::models::session::SessionExitReason::OperatorStop)
    );
}
