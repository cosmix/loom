//! Approval over a record with no PID identity.

use super::*;

/// A worker record with no PID identity, the state manual mode leaves by
/// design, stops refusing approval once no launch can still produce a process.
#[test]
fn test_human_review_approve_proceeds_past_an_old_record_with_no_identity() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    setup_stage(&temp, StageStatus::NeedsHumanReview, Some("Manual run"));
    let tmux = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(work_dir, &tmux).unwrap();
    let mut worker = Session::new();
    worker.assign_to_stage("test-stage".to_string());
    worker.backend = SessionBackendKind::Tmux;
    save_session(&worker, work_dir).unwrap();

    let message = format!("{:#}", handle_approve("test-stage", work_dir).unwrap_err());
    assert!(message.contains("no PID identity evidence"), "{message}");

    worker.created_at = chrono::Utc::now() - chrono::Duration::seconds(300);
    save_session(&worker, work_dir).unwrap();
    handle_approve("test-stage", work_dir).unwrap();
    assert_eq!(
        load_stage("test-stage", work_dir).unwrap().status,
        StageStatus::Queued
    );
}
