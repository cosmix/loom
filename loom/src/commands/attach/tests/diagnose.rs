use super::*;
use crate::orchestrator::terminal::tmux::viewer::tests::stub_session;
use chrono::Utc;

/// A stage with every field defaulted except `id` and `status` — sufficient
/// for `orphaned_stage_ids`, which reads only those two.
fn stub_stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        status,
        ..Stage::default()
    }
}

#[test]
fn attach_rejection_reason_names_a_non_tmux_backend() {
    let mut session = stub_session("session-a", "stage-x", Utc::now());
    session.backend = SessionBackendKind::Native;
    let reason = attach_rejection_reason(&session, |_| Ok(true));
    assert_eq!(reason, "backend is native");
}

#[test]
fn attach_rejection_reason_names_a_non_live_status() {
    let mut session = stub_session("session-a", "stage-x", Utc::now());
    session.backend = SessionBackendKind::Tmux;
    session.status = SessionStatus::Completed;
    let reason = attach_rejection_reason(&session, |_| Ok(true));
    assert_eq!(reason, "status is Completed");
}

#[test]
fn attach_rejection_reason_names_a_dead_process() {
    let mut session = stub_session("session-a", "stage-x", Utc::now());
    session.backend = SessionBackendKind::Tmux;
    session.status = SessionStatus::Running;
    let reason = attach_rejection_reason(&session, |_| Ok(false));
    assert_eq!(reason, "process is not running");
}

#[test]
fn attach_rejection_reason_names_a_missing_tracking_key() {
    // Alive-with-no-tracking-key cannot occur for real: both derive from the
    // same `window_title_and_pid_key` call (see `viewer::tmux_session_name`'s
    // doc comment). This proves the defensive branch still explains itself
    // instead of silently misreporting the session as live if that
    // invariant is ever broken.
    let mut session = Session::new();
    session.id = "session-a".to_string();
    session.backend = SessionBackendKind::Tmux;
    session.status = SessionStatus::Running;
    let reason = attach_rejection_reason(&session, |_| Ok(true));
    assert_eq!(reason, "no tracking key");
}

#[test]
fn orphaned_stage_ids_finds_executing_stages_with_no_naming_session() {
    let stages = vec![
        stub_stage("orphan", StageStatus::Executing),
        stub_stage("tracked", StageStatus::Executing),
        stub_stage("waiting", StageStatus::WaitingForDeps),
    ];
    let sessions = vec![stub_session("session-a", "tracked", Utc::now())];

    let orphans = orphaned_stage_ids(&stages, &sessions);

    assert_eq!(orphans, vec!["orphan".to_string()]);
}

#[test]
fn orphaned_stage_ids_is_empty_when_every_executing_stage_has_a_session() {
    let stages = vec![stub_stage("tracked", StageStatus::Executing)];
    let sessions = vec![stub_session("session-a", "tracked", Utc::now())];

    assert!(orphaned_stage_ids(&stages, &sessions).is_empty());
}

#[test]
fn no_session_records_message_names_the_sessions_dir() {
    let work_dir = std::path::Path::new("/tmp/example-repo/.work");
    let message = no_session_records_message(work_dir);
    assert!(
        message.contains("/tmp/example-repo/.work/sessions"),
        "{message}"
    );
}

#[test]
fn record_rejection_line_names_the_session_stage_and_reason() {
    let session = stub_session("session-a", "stage-x", Utc::now());
    let line = record_rejection_line(&session, "status is Completed");
    assert!(line.contains("session-a"), "{line}");
    assert!(line.contains("stage-x"), "{line}");
    assert!(line.contains("status is Completed"), "{line}");
}

#[test]
fn orphaned_stage_line_names_the_stage_and_both_recovery_paths() {
    let line = orphaned_stage_line("my-stage");
    assert!(line.contains("my-stage"), "{line}");
    assert!(line.contains("daemon will adopt"), "{line}");
    assert!(
        line.contains("loom stage reset --kill-session my-stage"),
        "{line}"
    );
}
