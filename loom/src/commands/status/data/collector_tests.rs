use super::*;
use crate::commands::status::data::{completion_blocker_summary, CompletionBlockerState};
use crate::handoff::{
    CompletionAttemptEvidence, CompletionBlocker, CompletionCheckpoint, CompletionPhase,
    CriterionResult, HandoffOrigin, HandoffV2, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::{SessionExitReason, SessionStatus};
use crate::parser::frontmatter::parse_from_markdown;

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

#[test]
fn live_merge_resolver_ignores_a_terminal_merge_session() {
    let (_tmp, work_dir) = temp_work_dir();
    let stage = make_test_stage("stage-1", StageStatus::MergeConflict);
    let mut finished = Session::new();
    finished.id = "resolver-old".to_string();
    finished.session_type = SessionType::Merge;
    finished.stage_id = Some("stage-1".to_string());
    finished.status = SessionStatus::Completed;
    let mut live = finished.clone();
    live.id = "resolver-live".to_string();
    live.status = SessionStatus::Running;

    let only_finished = build_stage_summary(&stage, &[finished.clone()], &work_dir);
    let with_live = build_stage_summary(&stage, &[finished, live], &work_dir);

    assert_eq!(only_finished.merge_resolver_session, None);
    assert_eq!(
        with_live.merge_resolver_session.as_deref(),
        Some("resolver-live")
    );
    assert_eq!(with_live.merge_resolver_attempts, Some(0));
}

#[test]
fn merge_resolver_facts_cross_the_daemon_wire_for_merge_stages_only() {
    let (_tmp, work_dir) = temp_work_dir();
    let mut resolver = Session::new();
    resolver.session_type = SessionType::Merge;
    resolver.stage_id = Some("stage-1".to_string());
    resolver.status = SessionStatus::Running;
    let sessions = [resolver.clone()];
    let merging = make_test_stage("stage-1", StageStatus::MergeBlocked);
    let queued = make_test_stage("stage-1", StageStatus::Queued);
    let merging = build_stage_summary(&merging, &sessions, &work_dir);
    let queued = build_stage_summary(&queued, &sessions, &work_dir);

    let received: StageSummary =
        serde_json::from_str(&serde_json::to_string(&merging).unwrap()).unwrap();
    let queued_wire = serde_json::to_value(&queued).unwrap();

    assert_eq!(received.merge_resolver_session, Some(resolver.id));
    assert_eq!(received.merge_resolver_attempts, Some(0));
    assert!(queued_wire.get("merge_resolver_session").is_none());
    assert!(queued_wire.get("merge_resolver_attempts").is_none());
}

/// Every directory (`None`) and file (its bytes) under `dir`, so a test can
/// prove that collecting status left the tree exactly as it found it.
fn tree(dir: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Option<Vec<u8>>> {
    let mut entries = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            entries.extend(tree(&path));
            entries.insert(path, None);
        } else {
            let bytes = std::fs::read(&path).unwrap();
            entries.insert(path, Some(bytes));
        }
    }
    entries
}

#[test]
fn merge_resolver_attempts_are_read_from_the_counter_file_without_writing() {
    let (_tmp, work_dir) = temp_work_dir();
    let counters = work_dir.root().join("merge-resolver-attempts");
    std::fs::create_dir_all(&counters).unwrap();
    std::fs::write(counters.join("stage-1.count"), "2").unwrap();
    let before = tree(work_dir.root());
    let stage = make_test_stage("stage-1", StageStatus::MergeBlocked);

    let summary = build_stage_summary(&stage, &[], &work_dir);

    assert_eq!(summary.merge_resolver_attempts, Some(2));
    assert_eq!(tree(work_dir.root()), before);
}

#[test]
fn a_merge_session_whose_process_died_is_not_the_live_resolver() {
    let (_tmp, work_dir) = temp_work_dir();
    let stage = make_test_stage("stage-1", StageStatus::MergeConflict);
    let mut dead = Session::new();
    dead.session_type = SessionType::Merge;
    dead.stage_id = Some("stage-1".to_string());
    dead.status = SessionStatus::Running;
    // Above Linux's pid_max, so no process holds it (see `process::tests`).
    dead.pid = Some(999_999_999);
    let mut live = dead.clone();
    live.pid = Some(std::process::id());

    let with_dead = build_stage_summary(&stage, &[dead], &work_dir);
    let with_live = build_stage_summary(&stage, &[live.clone()], &work_dir);

    assert_eq!(with_dead.merge_resolver_session, None);
    assert_eq!(with_live.merge_resolver_session, Some(live.id));
}

#[test]
fn stage_summary_carries_the_block_reason() {
    let (_tmp, work_dir) = temp_work_dir();
    let mut blocked = make_test_stage("stage-1", StageStatus::Blocked);
    blocked.close_reason = Some("needs an upstream schema change".to_string());
    let unblocked = make_test_stage("stage-2", StageStatus::Blocked);

    let summary = build_stage_summary(&blocked, &[], &work_dir);

    assert_eq!(
        summary.close_reason,
        Some("needs an upstream schema change".to_string())
    );
    assert_eq!(
        build_stage_summary(&unblocked, &[], &work_dir).close_reason,
        None
    );
}

#[test]
fn stage_summary_leaves_out_the_close_reason_of_a_stage_that_is_not_blocked() {
    let (_tmp, work_dir) = temp_work_dir();
    let mut skipped = make_test_stage("stage-1", StageStatus::Skipped);
    skipped.close_reason = Some("Not needed".to_string());

    assert_eq!(
        build_stage_summary(&skipped, &[], &work_dir).close_reason,
        None
    );
}
