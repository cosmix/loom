use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::dispute::dispute_dir;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::contracts::store::{attempts_spent, spend_attempt};
use crate::verify::contracts::test_support::contract_stage;
use tempfile::TempDir;

fn setup_stage(temp: &TempDir, status: StageStatus, review_reason: Option<&str>) -> Stage {
    let stages_dir = temp.path().join("stages");
    std::fs::create_dir_all(&stages_dir).unwrap();

    let stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status,
        review_reason: review_reason.map(|s| s.to_string()),
        fix_attempts: 5,
        ..Default::default()
    };

    crate::verify::transitions::save_stage(&stage, temp.path()).unwrap();
    stage
}

#[test]
fn test_human_review_approve() {
    let temp = TempDir::new().unwrap();
    setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Bad criteria"));

    let work_dir = temp.path();
    let mut stage = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(stage.fix_attempts, 5);

    stage.try_approve_review().unwrap();
    stage.fix_attempts = 0;

    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.fix_attempts, 0);
    assert_eq!(stage.review_reason, None);
}

#[test]
fn test_human_review_approve_resets_an_unfrozen_stage_contract_budget() {
    let temp = TempDir::new().unwrap();
    let mut stage = contract_stage("test-stage", "writer-1");
    stage.status = StageStatus::NeedsHumanReview;
    crate::verify::transitions::save_stage(&stage, temp.path()).unwrap();
    for _ in 0..3 {
        spend_attempt(temp.path(), "test-stage").unwrap();
    }
    assert_eq!(attempts_spent(temp.path(), "test-stage").unwrap(), 3);

    handle_approve("test-stage", temp.path()).unwrap();

    assert_eq!(
        load_stage("test-stage", temp.path()).unwrap().status,
        StageStatus::Queued
    );
    assert_eq!(attempts_spent(temp.path(), "test-stage").unwrap(), 0);
}

/// A refused transition (on-disk status no longer `NeedsHumanReview` by
/// the time the locked closure re-reads it) must not reset the budget:
/// the reset lives inside the same closure, after `try_approve_review`
/// re-validates, so its error path is never reached.
#[test]
fn test_human_review_approve_refused_transition_leaves_budget_unspent() {
    let temp = TempDir::new().unwrap();
    let mut stage = contract_stage("test-stage", "writer-1");
    // On-disk status is Completed, not NeedsHumanReview: `try_approve_review`'s
    // inner `NeedsHumanReview -> Queued` transition refuses it (Completed is
    // terminal, mirroring `test_human_review_wrong_state`).
    stage.status = StageStatus::Completed;
    crate::verify::transitions::save_stage(&stage, temp.path()).unwrap();
    for _ in 0..3 {
        spend_attempt(temp.path(), "test-stage").unwrap();
    }
    assert_eq!(attempts_spent(temp.path(), "test-stage").unwrap(), 3);

    let result = handle_approve("test-stage", temp.path());

    assert!(result.is_err());
    assert_eq!(
        load_stage("test-stage", temp.path()).unwrap().status,
        StageStatus::Completed
    );
    assert_eq!(attempts_spent(temp.path(), "test-stage").unwrap(), 3);
}

/// Approval settles the disputes the escalation left open, so the one a
/// fresh session files next is the only live dispute of the stage.
#[test]
fn test_human_review_approve_closes_an_open_dispute() {
    let temp = TempDir::new().unwrap();
    setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Escalated"));
    let dispute = dispute_dir(&temp.path().join("disputes"), "test-stage", 1);
    std::fs::create_dir_all(&dispute).unwrap();

    handle_approve("test-stage", temp.path()).unwrap();

    assert!(dispute.join("closed.marker").exists());
}

/// A stage not awaiting review keeps its disputes open: a refused approve
/// must not settle anything.
#[test]
fn test_human_review_refused_approve_leaves_disputes_open() {
    let temp = TempDir::new().unwrap();
    setup_stage(&temp, StageStatus::Completed, None);
    let dispute = dispute_dir(&temp.path().join("disputes"), "test-stage", 1);
    std::fs::create_dir_all(&dispute).unwrap();

    assert!(handle_approve("test-stage", temp.path()).is_err());

    assert!(!dispute.join("closed.marker").exists());
}

/// Approval queues the stage, and the executor adopts a live worker
/// session instead of spawning a fresh one, so approval refuses while one
/// is live. The test process stands in for the agent: alive throughout,
/// and nothing is left running after the test.
#[test]
fn test_human_review_approve_refuses_while_a_worker_session_is_live() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Agent live"));
    let tmux = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(work_dir, &tmux).unwrap();
    let mut worker = Session::new();
    worker.assign_to_stage("test-stage".to_string());
    worker.status = SessionStatus::Running;
    worker.backend = SessionBackendKind::Tmux;
    save_session(&worker, work_dir).unwrap();
    write_test_pid_identity(work_dir, &worker, std::process::id()).unwrap();

    let message = format!("{:#}", handle_approve("test-stage", work_dir).unwrap_err());
    assert!(message.contains(&worker.id), "{message}");
    assert!(
        message.contains("loom stage reset test-stage --kill-session"),
        "{message}"
    );
    let refused = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(refused.status, StageStatus::NeedsHumanReview);

    // Gone, as a takedown records it.
    worker.status = SessionStatus::ContextExhausted;
    save_session(&worker, work_dir).unwrap();
    handle_approve("test-stage", work_dir).unwrap();
    let approved = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(approved.status, StageStatus::Queued);
}

#[test]
fn test_human_review_reject() {
    let temp = TempDir::new().unwrap();
    setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Bad criteria"));

    let work_dir = temp.path();
    let mut stage = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);

    stage
        .try_reject_review("Not needed anymore".to_string())
        .unwrap();

    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(stage.review_reason, Some("Not needed anymore".to_string()));
}

/// A reject blocks the stage for a reason the operator gave, so a prior
/// attempt's failure record (a crash, a provision error) must not outlive
/// it and make the Blocked stage read as a crash.
#[test]
fn test_handle_reject_clears_a_prior_attempts_failure_info() {
    use crate::models::failure::{FailureInfo, FailureType};

    let temp = TempDir::new().unwrap();
    setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Bad criteria"));
    update_stage("test-stage", temp.path(), |stage| {
        stage.failure_info = Some(FailureInfo {
            failure_type: FailureType::SessionCrash,
            detected_at: chrono::Utc::now(),
            evidence: vec!["an earlier attempt".to_string()],
        });
        Ok(())
    })
    .unwrap();

    handle_reject("test-stage", "Not needed anymore", temp.path()).unwrap();

    let rejected = load_stage("test-stage", temp.path()).unwrap();
    assert_eq!(rejected.status, StageStatus::Blocked);
    assert_eq!(rejected.close_reason.as_deref(), Some("Not needed anymore"));
    assert!(rejected.failure_info.is_none());
}

#[test]
fn test_human_review_wrong_state() {
    // Completed is terminal: try_approve_review's inner NeedsHumanReview
    // -> Queued transition must refuse it regardless of the
    // command-level NeedsHumanReview check tested elsewhere.
    let mut stage = Stage {
        status: StageStatus::Completed,
        ..Default::default()
    };
    let result = stage.try_approve_review();
    assert!(result.is_err());
}

/// Approving a parked stage hands it to a fresh session, so the stall
/// recovery budget the previous session spent starts over.
#[test]
fn test_human_review_approve_resets_stall_recoveries() {
    let temp = TempDir::new().unwrap();
    setup_stage(&temp, StageStatus::NeedsHumanReview, Some("stalled"));
    update_stage("test-stage", temp.path(), |stage| {
        stage.stall_recoveries = 2;
        Ok(())
    })
    .unwrap();

    handle_approve("test-stage", temp.path()).unwrap();

    let approved = load_stage("test-stage", temp.path()).unwrap();
    assert_eq!(approved.status, StageStatus::Queued);
    assert_eq!(approved.stall_recoveries, 0);
}
