use std::collections::HashSet;
use std::fs;

use chrono::{DateTime, Utc};
use tempfile::TempDir;

use super::*;
use crate::handoff::{
    load_session_checkpoint, record_attempt_handoff, CompletionAttemptEvidence, CompletionPhase,
    CriterionResult, HandoffOrigin, HandoffV2, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};

const COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OBSERVED_AT: &str = "2026-09-14T10:00:00Z";

fn fixture() -> (TempDir, Session, Stage) {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("handoffs")).unwrap();
    let mut session = Session::new();
    session.id = "session-monitor".to_string();
    let mut stage = Stage::new("completion monitor".to_string(), None);
    stage.id = "stage-monitor".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    stage.subagent_timeout_secs = Some(60);
    (temp, session, stage)
}

fn evidence(
    session: &Session,
    stage: &Stage,
    nonce: &str,
    phase: CompletionPhase,
) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        commit: COMMIT.to_string(),
        check_definition_hash: "b".repeat(64),
        exact_command: "cargo test --lib".to_string(),
        evidence_nonce: nonce.to_string(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "criterion-1".to_string(),
                passed: true,
            }],
            environment_policy: "stage-host-allowlist-v2".to_string(),
            environment: Vec::new(),
        },
        phase,
        external_failure_code: matches!(
            phase,
            CompletionPhase::VerifiedPendingAck | CompletionPhase::DaemonRejected
        )
        .then(|| "daemon_offline".to_string()),
        diagnostic_first_line: None,
        observed_at: OBSERVED_AT.to_string(),
        attestation: None,
    }
}

fn scan(
    watch: &mut CompletionBlockerWatch,
    stage: &Stage,
    work_dir: &std::path::Path,
    now: DateTime<Utc>,
    commit: Option<&str>,
) -> BlockerScan {
    watch.scan_with(std::slice::from_ref(stage), work_dir, now, |_| {
        commit.map(str::to_string)
    })
}

fn now(seconds: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(OBSERVED_AT)
        .unwrap()
        .with_timezone(&Utc)
        + chrono::Duration::seconds(seconds)
}

fn fingerprint(stage: &Stage, session: &Session, work_dir: &std::path::Path) -> String {
    load_session_checkpoint(&stage.id, &session.id, work_dir)
        .unwrap()
        .unwrap()
        .blocker
        .unwrap()
        .fingerprint
}

#[test]
fn first_failure_emits_pending_once() {
    let (temp, session, stage) = fixture();
    let attempt = evidence(
        &session,
        &stage,
        "evidence_nonce_000000001",
        CompletionPhase::VerifiedPendingAck,
    );
    record_attempt_handoff(&session, &stage, &attempt, temp.path()).unwrap();
    let expected = MonitorEvent::CompletionPending {
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        fingerprint: fingerprint(&stage, &session, temp.path()),
        repeat_count: 1,
    };
    let mut watch = CompletionBlockerWatch::default();

    let first = scan(&mut watch, &stage, temp.path(), now(1), Some(COMMIT));
    let second = scan(&mut watch, &stage, temp.path(), now(2), Some(COMMIT));

    assert_eq!((first.events, second.events), (vec![expected], Vec::new()));
}

#[test]
fn second_distinct_nonce_emits_repeated_blocker() {
    let (temp, session, stage) = fixture();
    let first = evidence(
        &session,
        &stage,
        "evidence_nonce_000000001",
        CompletionPhase::VerifiedPendingAck,
    );
    record_attempt_handoff(&session, &stage, &first, temp.path()).unwrap();
    let mut watch = CompletionBlockerWatch::default();
    scan(&mut watch, &stage, temp.path(), now(1), Some(COMMIT));
    let second = evidence(
        &session,
        &stage,
        "evidence_nonce_000000002",
        CompletionPhase::VerifiedPendingAck,
    );
    record_attempt_handoff(&session, &stage, &second, temp.path()).unwrap();
    let expected = MonitorEvent::CompletionBlocked {
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        fingerprint: fingerprint(&stage, &session, temp.path()),
        repeat_count: 2,
        escalation: CompletionEscalation::Repeated,
    };

    let result = scan(&mut watch, &stage, temp.path(), now(2), Some(COMMIT));

    assert_eq!(result.events, vec![expected]);
}

#[test]
fn old_first_failure_expires_idle_budget() {
    let (temp, session, stage) = fixture();
    let attempt = evidence(
        &session,
        &stage,
        "evidence_nonce_000000001",
        CompletionPhase::VerifiedPendingAck,
    );
    record_attempt_handoff(&session, &stage, &attempt, temp.path()).unwrap();
    let expected = MonitorEvent::CompletionBlocked {
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        fingerprint: fingerprint(&stage, &session, temp.path()),
        repeat_count: 1,
        escalation: CompletionEscalation::IdleBudgetExpired,
    };

    let result = scan(
        &mut CompletionBlockerWatch::default(),
        &stage,
        temp.path(),
        now(61),
        Some(COMMIT),
    );

    assert_eq!(result.events, vec![expected]);
}

#[test]
fn exhausted_checkpoint_uses_typed_capacity_escalation() {
    let (_temp, session, stage) = fixture();
    let mut checkpoint = CompletionCheckpoint::new(&stage.id, &session.id);
    checkpoint.capacity_exhausted = true;

    let result = classify(&checkpoint, &stage, None, now(1));

    assert_eq!(
        result,
        Some(Observation {
            session_id: session.id,
            fingerprint: "capacity-exhausted".to_string(),
            repeat_count: 0,
            escalation: Some(CompletionEscalation::CapacityExhausted),
        })
    );
}

#[test]
fn changed_session_or_commit_is_not_current() {
    let (temp, session, stage) = fixture();
    let attempt = evidence(
        &session,
        &stage,
        "evidence_nonce_000000001",
        CompletionPhase::VerifiedPendingAck,
    );
    record_attempt_handoff(&session, &stage, &attempt, temp.path()).unwrap();
    let mut changed_session = stage.clone();
    changed_session.session = Some("another-session".to_string());

    let wrong_session = scan(
        &mut CompletionBlockerWatch::default(),
        &changed_session,
        temp.path(),
        now(1),
        Some(COMMIT),
    );
    let wrong_commit = scan(
        &mut CompletionBlockerWatch::default(),
        &stage,
        temp.path(),
        now(1),
        Some("cccccccccccccccccccccccccccccccccccccccc"),
    );

    assert_eq!(
        (wrong_session.events, wrong_commit.events),
        (Vec::new(), Vec::new())
    );
}

#[test]
fn tool_failure_without_verified_external_code_is_ignored() {
    let (temp, session, stage) = fixture();
    let attempt = evidence(
        &session,
        &stage,
        "evidence_nonce_000000001",
        CompletionPhase::ToolFailed,
    );
    record_attempt_handoff(&session, &stage, &attempt, temp.path()).unwrap();

    let result = scan(
        &mut CompletionBlockerWatch::default(),
        &stage,
        temp.path(),
        now(61),
        Some(COMMIT),
    );

    assert!(result.events.is_empty());
}

#[test]
fn blocker_owned_hung_event_is_filtered_without_affecting_others() {
    let hung = |stage_id: &str| MonitorEvent::SessionHung {
        session_id: format!("session-{stage_id}"),
        stage_id: Some(stage_id.to_string()),
        stale_duration_secs: 61,
        timeout_secs: 60,
        last_activity: None,
        finished_without_completing: false,
    };
    let retained = hung("free");
    let mut events = vec![hung("owned"), retained.clone()];

    filter_owned_hung_events(&mut events, &HashSet::from(["owned".to_string()]));

    assert_eq!(events, vec![retained]);
}

#[test]
fn unreadable_handoff_emits_no_completion_event() {
    let (temp, _session, stage) = fixture();
    fs::create_dir(temp.path().join("handoffs/stage-monitor-handoff-001.md")).unwrap();

    let result = scan(
        &mut CompletionBlockerWatch::default(),
        &stage,
        temp.path(),
        now(61),
        Some(COMMIT),
    );

    assert!(result.events.is_empty());
}

#[test]
fn unattested_checkpoint_emits_no_completion_event() {
    let (temp, session, stage) = fixture();
    let mut checkpoint = CompletionCheckpoint::new(&stage.id, &session.id);
    checkpoint
        .record_attempt(&evidence(
            &session,
            &stage,
            "evidence_nonce_000000001",
            CompletionPhase::VerifiedPendingAck,
        ))
        .unwrap();
    let handoff = HandoffV2::new(&session.id, &stage.id)
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint));
    fs::write(
        temp.path().join("handoffs/stage-monitor-handoff-001.md"),
        format!("---\n{}---\n", handoff.to_yaml().unwrap()),
    )
    .unwrap();

    let result = scan(
        &mut CompletionBlockerWatch::default(),
        &stage,
        temp.path(),
        now(61),
        Some(COMMIT),
    );

    assert!(result.events.is_empty());
}
