use super::super::tests::spawn_orphan_process;
use super::park_tests::{assert_ownership_unknown, block, fixture};
use crate::fs::session_files::load_session_exact;
use crate::models::session::SessionStatus;
use crate::models::stage::StageStatus;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::subagent_lifecycle::model::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};
use crate::subagent_lifecycle::store::{append_locked, codex_event_id, replay, ChildDisposition};
use std::path::Path;

fn child_evidence(evidence_kind: CodexEvidenceKind, terminal: bool) -> CodexEvidence {
    CodexEvidence {
        evidence_kind,
        requested_model: "gpt-5.6-sol".into(),
        requested_effort: "high".into(),
        invocation_id: "invocation-child".into(),
        job_id: Some("job-child".into()),
        thread_id: terminal.then(|| "thread-child".into()),
        turn_id: terminal.then(|| "turn-child".into()),
        tool_use_id: None,
        terminal_at: terminal.then(|| "2026-09-14T10:00:00Z".parse().unwrap()),
        outcome: if terminal {
            CodexEvidenceOutcome::Succeeded
        } else {
            CodexEvidenceOutcome::Running
        },
        detail: None,
    }
}

fn child_record(
    repo: &Path,
    session_id: &str,
    terminal: bool,
    evidence: &CodexEvidence,
) -> LifecycleRecord {
    LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::CodexCompanion,
        identity: WorkerIdentity::Codex {
            stage_id: "test-stage".into(),
            loom_session_id: session_id.into(),
            parent_session_id: "parent-session".into(),
            forwarder_agent_id: "forwarder-child".into(),
            unit_id: "unit-child".into(),
            invocation_id: "invocation-child".into(),
            workspace_root: std::fs::canonicalize(repo).unwrap(),
            execution: CodexExecution::Companion {
                job_id: "job-child".into(),
            },
        },
        observed_at: "2026-09-14T10:00:00Z".parse().unwrap(),
        state: if terminal {
            LifecycleState::Completed
        } else {
            LifecycleState::Running
        },
        evidence: serde_json::to_value(evidence).unwrap(),
    }
}

fn append_child(work: &Path, mut record: LifecycleRecord, evidence: &CodexEvidence) {
    record.event_id = codex_event_id(&record, evidence).unwrap();
    append_locked(work, &record).unwrap();
}

fn active_child(work: &Path, repo: &Path, session_id: &str) {
    for kind in [
        CodexEvidenceKind::Authorization,
        CodexEvidenceKind::Observation,
    ] {
        let evidence = child_evidence(kind, false);
        append_child(
            work,
            child_record(repo, session_id, false, &evidence),
            &evidence,
        );
    }
}

fn complete_child(work: &Path, repo: &Path, session_id: &str) {
    let evidence = child_evidence(CodexEvidenceKind::Observation, true);
    append_child(
        work,
        child_record(repo, session_id, true, &evidence),
        &evidence,
    );
}

fn corrupt_child_journal(work: &Path) {
    let dir = work.join("subagents/test-stage");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("lifecycle.jsonl"), "{bad}\n").unwrap();
}

#[test]
fn active_child_forbids_takedown() {
    let mut fixture = fixture(2);
    let pid = spawn_orphan_process();
    write_test_pid_identity(&fixture.work, &fixture.session, pid).unwrap();
    active_child(&fixture.work, fixture._temp.path(), &fixture.session.id);
    block(&mut fixture);

    assert!(crate::process::is_process_alive(pid));
    assert_eq!(
        (
            crate::verify::transitions::load_stage("test-stage", &fixture.work)
                .unwrap()
                .status,
            load_session_exact(&fixture.work, &fixture.session.id)
                .unwrap()
                .unwrap()
                .status
        ),
        (StageStatus::Executing, SessionStatus::Running)
    );
    assert_eq!(
        load_session_exact(&fixture.work, &fixture.session.id)
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Running
    );
    crate::process::terminate(pid).unwrap();
}

#[test]
fn unknown_child_escalates_without_kill() {
    let mut fixture = fixture(2);
    let pid = spawn_orphan_process();
    write_test_pid_identity(&fixture.work, &fixture.session, pid).unwrap();
    corrupt_child_journal(&fixture.work);
    block(&mut fixture);

    assert!(crate::process::is_process_alive(pid));
    assert_ownership_unknown(&fixture);
    crate::process::terminate(pid).unwrap();
}

#[test]
fn lifecycle_disposition_covers_no_children_active_and_unknown_journals() {
    let fixture = fixture(1);
    active_child(&fixture.work, fixture._temp.path(), &fixture.session.id);
    assert_eq!(
        replay(&fixture.work)
            .unwrap()
            .session_child_disposition("test-stage", &fixture.session.id),
        ChildDisposition::Active
    );
    complete_child(&fixture.work, fixture._temp.path(), &fixture.session.id);
    assert_eq!(
        replay(&fixture.work)
            .unwrap()
            .session_child_disposition("test-stage", &fixture.session.id),
        ChildDisposition::NoChildren
    );
    corrupt_child_journal(&fixture.work);
    assert!(matches!(
        replay(&fixture.work)
            .unwrap()
            .session_child_disposition("test-stage", &fixture.session.id),
        ChildDisposition::Unknown(_)
    ));
}
