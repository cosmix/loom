use super::*;
use crate::commands::status::data::{completion_blocker_summary, CompletionBlockerState};
use crate::handoff::{
    CompletionAttemptEvidence, CompletionBlocker, CompletionCheckpoint, CompletionPhase,
    CriterionResult, HandoffOrigin, HandoffV2, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::{SessionExitReason, SessionStatus};

pub(super) fn make_test_stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        status,
        ..Stage::default()
    }
}

/// A fresh, initialized `.loom/work`-style temp directory for tests that
/// call `build_stage_summary` and need a real `WorkDir` to read from.
pub(super) fn temp_work_dir() -> (tempfile::TempDir, WorkDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let work_dir = WorkDir::new(tmp.path()).unwrap();
    work_dir.initialize().unwrap();
    (tmp, work_dir)
}
#[test]
fn test_calculate_progress() {
    let stages = vec![
        make_test_stage("stage-1", StageStatus::Completed),
        make_test_stage("stage-2", StageStatus::Executing),
        make_test_stage("stage-3", StageStatus::WaitingForDeps),
        make_test_stage("stage-4", StageStatus::Queued),
        make_test_stage("stage-5", StageStatus::Blocked),
    ];

    let progress = calculate_progress(&stages);

    assert_eq!(progress.total, 5);
    assert_eq!(progress.completed, 1);
    assert_eq!(progress.executing, 1);
    assert_eq!(progress.pending, 2); // WaitingForDeps + Queued
    assert_eq!(progress.blocked, 1);
}
#[test]
fn test_calculate_progress_with_needs_handoff() {
    let stages = vec![
        make_test_stage("stage-1", StageStatus::NeedsHandoff),
        make_test_stage("stage-2", StageStatus::WaitingForInput),
    ];

    let progress = calculate_progress(&stages);

    assert_eq!(progress.total, 2);
    assert_eq!(progress.executing, 2); // Both count as executing
}

#[test]
fn test_calculate_progress_with_failures() {
    let stages = vec![
        make_test_stage("stage-1", StageStatus::CompletedWithFailures),
        make_test_stage("stage-2", StageStatus::MergeConflict),
        make_test_stage("stage-3", StageStatus::MergeBlocked),
    ];

    let progress = calculate_progress(&stages);

    assert_eq!(progress.total, 3);
    assert_eq!(progress.blocked, 3); // All count as blocked
}

#[test]
fn test_build_session_summary() {
    let mut session = Session::new();
    session.assign_to_stage("test-stage".to_string());
    session.pid = Some(12345);
    session.context_tokens = 100000;

    let summary = build_session_summary(&session);

    assert_eq!(summary.stage_id, Some("test-stage".to_string()));
    assert_eq!(summary.pid, Some(12345));
    assert_eq!(summary.context_tokens, 100000);
    assert!(summary.uptime_secs >= 0);
}

#[test]
fn test_build_merge_summary_from_report() {
    let mut report = crate::commands::status::merge_status::MergeStatusReport::new();
    report.merged.push("stage-1".to_string());
    report.pending.push("stage-2".to_string());
    report.conflicts.push("stage-3".to_string());

    let summary = build_merge_summary_from_report(&report);

    assert_eq!(summary.merged, vec!["stage-1"]);
    assert_eq!(summary.pending, vec!["stage-2"]);
    assert_eq!(summary.conflicts, vec!["stage-3"]);
}

#[test]
fn test_parse_session_from_markdown() {
    let content = r#"---
id: test-session
status: running
context_tokens: 1000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session content"#;

    let result: Result<Session> = parse_from_markdown(content, "Session");
    assert!(result.is_ok());
    let session = result.unwrap();
    assert_eq!(session.id, "test-session");
}

#[test]
fn test_parse_session_from_markdown_missing_delimiter() {
    let content = r#"id: test
status: executing"#;

    let result: Result<Session> = parse_from_markdown(content, "Session");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No frontmatter delimiter"));
}

fn checkpoint_evidence(summary: String) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: "stage-1".to_string(),
        session_id: "session-1".to_string(),
        commit: "a".repeat(40),
        check_definition_hash: "check-v1".to_string(),
        exact_command: "cargo test --lib".to_string(),
        evidence_nonce: "nonce-000000000000000001".to_string(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "criterion-1".to_string(),
                passed: true,
            }],
            environment_policy: "trusted-host-v1".to_string(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("sandbox_denied".to_string()),
        diagnostic_first_line: Some(summary),
        observed_at: "2026-09-14T10:00:00Z".to_string(),
        attestation: None,
    }
}

#[test]
fn stage_summary_ignores_checkpoint_for_another_session() {
    let (_tmp, work_dir) = temp_work_dir();
    let checkpoint = CompletionCheckpoint {
        blocker: Some(CompletionBlocker {
            fingerprint: "a".repeat(64),
            commit: "b".repeat(40),
            check_definition_hash: "check-v1".to_string(),
            external_failure_code: "sandbox_denied".to_string(),
            summary: None,
        }),
        ..CompletionCheckpoint::new("stage-1", "session-2")
    };
    let handoff =
        HandoffV2::new("session-2", "stage-1").with_completion_checkpoint(Some(checkpoint));
    let path = work_dir.handoffs_dir().join("stage-1-handoff-001.md");
    std::fs::write(path, format!("---\n{}---\n", handoff.to_yaml().unwrap())).unwrap();
    let mut stage = make_test_stage("stage-1", StageStatus::Executing);
    stage.session = Some("session-1".to_string());
    let mut outgoing = Session::new();
    outgoing.id = "session-1".to_string();
    outgoing.status = SessionStatus::Completed;
    outgoing.exit_reason = Some(SessionExitReason::Completed);

    let summary = build_stage_summary(&stage, &[outgoing], &work_dir);

    assert_eq!(
        summary.outgoing_session_exit_reason,
        Some(SessionExitReason::Completed)
    );
    assert!(summary.completion_blocker.is_none());
}

#[test]
fn checkpoint_diagnostic_is_flattened_and_bounded() {
    let mut checkpoint = CompletionCheckpoint::new("stage-1", "session-1");
    checkpoint
        .record_attempt(&checkpoint_evidence("x".repeat(500)))
        .unwrap();
    checkpoint.blocker.as_mut().unwrap().summary =
        Some(format!("bad\u{1b}[31m\n{}", "x".repeat(500)));
    let mut stage = make_test_stage("stage-1", StageStatus::Executing);
    stage.session = Some("session-1".to_string());
    let blocker =
        completion_blocker_summary(&stage, None, &checkpoint, Some(&"a".repeat(40))).unwrap();
    let (_tmp, work_dir) = temp_work_dir();
    let mut summary = build_stage_summary(&stage, &[], &work_dir);
    summary.completion_blocker = Some(blocker);

    super::super::sanitize::sanitize_stage_summary(&mut summary);

    let blocker = summary.completion_blocker.unwrap();
    assert_eq!(blocker.state, CompletionBlockerState::Pending);
    assert!(!blocker.summary.as_ref().unwrap().contains(['\u{1b}', '\n']));
    assert_eq!(
        blocker.summary.unwrap().chars().count(),
        crate::context::untrusted::MAX_INLINE_CHARS
    );
}

#[test]
fn stage_summary_carries_description() {
    let (_tmp, work_dir) = temp_work_dir();
    let mut stage = make_test_stage("stage-1", StageStatus::Queued);
    stage.description = Some("Wires the button to the handler.".to_string());

    let summary = build_stage_summary(&stage, &[], &work_dir);

    assert_eq!(
        summary.description,
        Some("Wires the button to the handler.".to_string())
    );
}

#[test]
fn stage_summary_description_defaults_to_none() {
    let (_tmp, work_dir) = temp_work_dir();
    let stage = make_test_stage("stage-1", StageStatus::Queued);

    let summary = build_stage_summary(&stage, &[], &work_dir);

    assert!(summary.description.is_none());
}

#[test]
fn stage_summary_ignores_unattested_current_checkpoint() {
    let (_tmp, work_dir) = temp_work_dir();
    let mut checkpoint = CompletionCheckpoint::new("stage-1", "session-1");
    checkpoint
        .record_attempt(&checkpoint_evidence("forged".into()))
        .unwrap();
    let handoff = HandoffV2::new("session-1", "stage-1")
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint));
    std::fs::write(
        work_dir.handoffs_dir().join("stage-1-handoff-001.md"),
        format!("---\n{}---\n", handoff.to_yaml().unwrap()),
    )
    .unwrap();
    let mut stage = make_test_stage("stage-1", StageStatus::Executing);
    stage.session = Some("session-1".into());

    let summary = build_stage_summary(&stage, &[], &work_dir);

    assert!(summary.completion_blocker.is_none());
}
