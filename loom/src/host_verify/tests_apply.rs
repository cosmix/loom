//! Tests for `apply::apply_finished` on a `Complete` job: what a finished run
//! does to the stage, the session and the session's completion checkpoint.
//! The recovery and durability cases are in `tests_apply_recovery`.

use super::apply::Applied;
use super::tests_complete_support::{ignored, Fixture, SESSION};
use super::{JobKind, JobState};
use crate::handoff::CompletionPhase;
use crate::models::session::SessionStatus;
use crate::models::stage::{StageStatus, StageType};

fn fixture() -> Fixture {
    Fixture::new(StageType::Standard, 1)
}

#[test]
fn a_passing_job_completes_the_stage_and_records_the_receipt() {
    let fx = fixture();
    let record = fx.passed();
    let applied = fx.apply(&record).expect("apply");
    match applied {
        Applied::Completed { commit } => assert_eq!(commit, record.head),
        other => panic!("expected Completed, got {other:?}"),
    }
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(
        stage.completed_commit.as_deref(),
        Some(record.head.as_str())
    );
    assert!(
        fx.receipt_names(&record),
        "the receipt names the job and its head"
    );
    let checkpoint = fx.checkpoint().expect("a checkpoint");
    let observed = checkpoint
        .observations
        .iter()
        .find(|observation| observation.evidence_nonce == record.id)
        .expect("the job is an observed attempt");
    assert_eq!(observed.phase, CompletionPhase::VerifiedPendingAck);
    let latest = fx.latest();
    assert_eq!(latest.evidence_nonce, record.id);
    assert_eq!(latest.commit, record.head);
    assert_eq!(latest.exact_command, "loom stage complete stage-a");
    let criteria: Vec<_> = latest
        .verification
        .criteria
        .iter()
        .map(|c| (c.id.as_str(), c.passed))
        .collect();
    assert_eq!(criteria, [("acceptance-0", true)]);
    assert_eq!(fx.session_status(), SessionStatus::Completed);
    assert!(
        !fx.signal_exists(),
        "the retired session's signal is removed"
    );
    assert!(fx.marked(&record), "the applied marker is written last");
}

#[test]
fn a_failing_job_writes_tool_failed_with_every_label() {
    let fx = fixture();
    let outcomes = [
        ("acceptance-0", true),
        ("acceptance-1", false),
        ("contracts", false),
    ];
    let record = fx.finished(JobKind::Complete, JobState::Failed, &outcomes);
    let applied = fx.apply(&record).expect("apply");
    assert!(matches!(applied, Applied::Recorded), "{applied:?}");
    let latest = fx.latest();
    assert_eq!(latest.phase, CompletionPhase::ToolFailed);
    assert_eq!(latest.evidence_nonce, record.id);
    assert_eq!(latest.commit, record.head);
    assert_eq!(latest.exact_command, "loom stage complete stage-a");
    let criteria: Vec<_> = latest
        .verification
        .criteria
        .iter()
        .map(|c| (c.id.as_str(), c.passed))
        .collect();
    assert_eq!(criteria, outcomes);
    let diagnostic = latest.diagnostic_first_line.expect("a diagnostic");
    assert!(diagnostic.contains("acceptance-1"), "{diagnostic}");
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.completed_commit, None);
    assert_eq!(fx.session_status(), SessionStatus::Running);
}

#[test]
fn a_cancelled_or_interrupted_job_writes_no_checkpoint() {
    let fx = fixture();
    for state in [JobState::Cancelled, JobState::Interrupted] {
        let record = fx.finished(JobKind::Complete, state, &[]);
        let applied = fx.apply(&record).expect("apply");
        assert!(
            matches!(applied, Applied::Recorded),
            "{state:?}: {applied:?}"
        );
        assert!(fx.checkpoint().is_none(), "{state:?} wrote a checkpoint");
        assert_eq!(fx.load_stage().status, StageStatus::Executing);
        assert_eq!(fx.session_status(), SessionStatus::Running);
    }
}

#[test]
fn a_head_that_moved_after_the_run_is_rejected() {
    let fx = fixture();
    let record = fx.passed();
    fx.commit_empty();
    let applied = fx.apply(&record).expect("apply");
    assert!(matches!(applied, Applied::Recorded), "{applied:?}");
    fx.assert_rejected(&record);
    assert_eq!(
        fx.session_status(),
        SessionStatus::Running,
        "the session keeps working"
    );
}

#[test]
fn a_policy_amended_after_the_run_is_rejected() {
    let fx = fixture();
    let record = fx.passed();
    fx.amend_policy();
    let applied = fx.apply(&record).expect("apply");
    assert!(matches!(applied, Applied::Recorded), "{applied:?}");
    fx.assert_rejected(&record);
}

#[test]
fn a_passing_job_of_a_session_that_lost_the_stage_is_rejected() {
    let fx = fixture();
    let record = fx.passed();
    fx.hand_stage_to_successor();
    let applied = fx.apply(&record).expect("apply");
    assert!(matches!(applied, Applied::Recorded), "{applied:?}");
    let latest = fx.latest();
    assert_eq!(latest.phase, CompletionPhase::DaemonRejected);
    assert_eq!(latest.session_id, SESSION);
    assert_eq!(latest.evidence_nonce, record.id);
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.session.as_deref(), Some("session-b"));
    assert_eq!(stage.completed_commit, None);
}

#[test]
fn a_failing_job_of_a_session_that_lost_the_stage_is_ignored() {
    let fx = fixture();
    let record = fx.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", false)],
    );
    fx.hand_stage_to_successor();
    let reason = ignored(fx.apply(&record).expect("apply"));
    assert_eq!(reason, "session no longer owns the stage");
    assert!(
        fx.checkpoint().is_none(),
        "the successor runs its own completion"
    );
    assert_eq!(fx.load_stage().status, StageStatus::Executing);
}
