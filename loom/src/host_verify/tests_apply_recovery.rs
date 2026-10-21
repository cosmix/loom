//! Tests for `apply::apply_finished` beyond the plain pass and fail: the
//! completion gates, knowledge stages, records that are not applied, a record
//! applied twice, a crash between the stage transition and the marker, and a
//! checkpoint directory that cannot be written.

use std::os::unix::fs::PermissionsExt;

use super::apply::Applied;
use super::tests_complete_support::{ignored, Fixture, SESSION, STAGE};
use super::{JobKind, JobState};
use crate::handoff::CompletionPhase;
use crate::models::session::SessionStatus;
use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::update_stage;

#[test]
fn a_v2_standard_stage_without_a_review_round_is_rejected() {
    let fx = Fixture::new(StageType::Standard, 2);
    let record = fx.passed();
    let applied = fx.apply(&record).expect("apply");
    assert!(matches!(applied, Applied::Recorded), "{applied:?}");
    let latest = fx.assert_rejected(&record);
    let diagnostic = latest.diagnostic_first_line.expect("a diagnostic");
    assert!(diagnostic.contains("review gate"), "{diagnostic}");
}

#[test]
fn a_knowledge_stage_completes_merged() {
    let fx = Fixture::new(StageType::Knowledge, 1);
    let record = fx.passed();
    let applied = fx.apply(&record).expect("apply");
    match applied {
        Applied::Completed { commit } => assert_eq!(commit, record.head),
        other => panic!("expected Completed, got {other:?}"),
    }
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged, "a knowledge stage has no branch to merge");
    assert_eq!(
        stage.completed_commit.as_deref(),
        Some(record.head.as_str())
    );
    assert!(fx.receipt_names(&record));
    assert_eq!(fx.session_status(), SessionStatus::Completed);
    assert!(fx.marked(&record));
}

#[test]
fn a_check_record_is_ignored() {
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.finished(JobKind::Check, JobState::Passed, &[("acceptance-0", true)]);
    ignored(fx.apply(&record).expect("apply"));
    assert_eq!(fx.load_stage().status, StageStatus::Executing);
    assert!(
        fx.checkpoint().is_none(),
        "a check never touches the checkpoint"
    );
    assert_eq!(fx.session_status(), SessionStatus::Running);
}

#[test]
fn an_applied_record_is_not_applied_twice() {
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.passed();
    assert!(matches!(fx.apply(&record), Ok(Applied::Completed { .. })));
    let checkpoint = serde_json::to_string(&fx.checkpoint()).unwrap();
    let stage = fx.load_stage();

    let reason = ignored(fx.apply(&record).expect("second apply"));
    assert_eq!(reason, "already applied");
    assert_eq!(serde_json::to_string(&fx.checkpoint()).unwrap(), checkpoint);
    assert_eq!(fx.load_stage().completed_commit, stage.completed_commit);
}

/// The state a crash leaves after the stage transition and before the
/// receipt: the stage `Completed` at the job's head, nothing else written.
fn complete_stage_only(fx: &Fixture, head: &str) {
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.completed_commit = Some(head.to_string());
        stage.try_complete(None)
    })
    .expect("complete the stage");
}

#[test]
fn a_crash_after_the_transition_is_finished_by_the_next_apply() {
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.passed();
    complete_stage_only(&fx, &record.head);
    assert!(fx.checkpoint().is_none());
    assert!(!fx.marked(&record));

    let reason = ignored(fx.apply(&record).expect("apply"));
    assert_eq!(reason, "already completed");
    assert!(fx.receipt_names(&record), "the receipt is recorded");
    assert_eq!(fx.session_status(), SessionStatus::Completed);
    assert!(!fx.signal_exists());
    assert!(fx.marked(&record));
    let checkpoint = fx.checkpoint().expect("a checkpoint");
    assert!(
        checkpoint
            .observations
            .iter()
            .all(|observation| observation.phase != CompletionPhase::DaemonRejected),
        "a completed stage is never reported as refused"
    );
    assert_ne!(fx.latest().phase, CompletionPhase::DaemonRejected);
    assert_eq!(fx.load_stage().status, StageStatus::Completed);
}

/// Damage the session record so reading it fails, as an I/O failure would.
fn corrupt_session_record(fx: &Fixture) {
    let path = fx.work_dir.join("sessions").join(format!("{SESSION}.md"));
    std::fs::write(path, "not a session record").unwrap();
}

#[test]
fn an_unreadable_session_record_is_an_error_not_a_rejection() {
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.passed();
    corrupt_session_record(&fx);

    assert!(fx.apply(&record).is_err(), "a read failure is retried");
    assert!(!fx.marked(&record), "a failed application leaves no marker");
    assert!(fx.checkpoint().is_none(), "no rejection is recorded");
    assert_eq!(fx.load_stage().status, StageStatus::Executing);
}

#[test]
fn an_unreadable_session_record_of_a_failed_run_is_an_error_not_ignored() {
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", false)],
    );
    corrupt_session_record(&fx);

    assert!(fx.apply(&record).is_err(), "a read failure is retried");
    assert!(!fx.marked(&record), "a failed application leaves no marker");
}

#[test]
fn an_unwritable_checkpoint_directory_is_an_error_until_it_is_restored() {
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return; // root writes any directory, so the failure cannot be injected
    }
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.passed();
    let handoffs = fx.work_dir.join("handoffs");
    std::fs::create_dir_all(&handoffs).unwrap();
    std::fs::set_permissions(&handoffs, std::fs::Permissions::from_mode(0o555)).unwrap();

    let failed = fx.apply(&record);

    std::fs::set_permissions(&handoffs, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        failed.is_err(),
        "an I/O failure is an error, not an outcome"
    );
    assert!(!fx.marked(&record), "a failed application leaves no marker");

    fx.apply(&record).expect("the retry completes");
    assert_eq!(fx.load_stage().status, StageStatus::Completed);
    assert!(fx.receipt_names(&record));
    assert_eq!(fx.session_status(), SessionStatus::Completed);
    assert!(fx.marked(&record));
}
