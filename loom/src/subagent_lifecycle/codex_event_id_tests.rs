//! The Codex event id is a persisted digest: its bytes must not change.

use std::path::PathBuf;

use super::model::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};
use super::store::codex_event_id;

#[test]
fn a_fixed_codex_record_keeps_its_event_id() {
    let record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::CodexCompanion,
        identity: WorkerIdentity::Codex {
            stage_id: "stage-a".into(),
            loom_session_id: "loom-session-a".into(),
            parent_session_id: "parent-uuid-a".into(),
            forwarder_agent_id: "forwarder-a".into(),
            unit_id: "unit-a".into(),
            invocation_id: "invocation-unit-a".into(),
            workspace_root: PathBuf::from("/work/space"),
            execution: CodexExecution::Companion {
                job_id: "job-a".into(),
            },
        },
        observed_at: "2026-09-13T10:00:02Z".parse().unwrap(),
        state: LifecycleState::Running,
        evidence: serde_json::Value::Null,
    };
    let evidence = CodexEvidence {
        evidence_kind: CodexEvidenceKind::Observation,
        requested_model: "gpt-6-sol".into(),
        requested_effort: "xhigh".into(),
        invocation_id: "invocation-unit-a".into(),
        job_id: Some("job-a".into()),
        thread_id: Some("thread-unit-a".into()),
        turn_id: Some("turn-unit-a".into()),
        tool_use_id: None,
        terminal_at: Some("2026-09-13T10:00:02Z".parse().unwrap()),
        outcome: CodexEvidenceOutcome::Succeeded,
        detail: None,
    };

    assert_eq!(
        codex_event_id(&record, &evidence).unwrap(),
        "sha256:be14f3384cb1057d53dc68706269f22659766dda2c309b634d5d78f875d333c0"
    );
}
