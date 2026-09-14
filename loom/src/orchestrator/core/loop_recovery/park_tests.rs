use super::super::tests::{
    executing_stage, handoff_work_dir, orchestrator_for, recorded_session, spawn_orphan_process,
    write_pid_file,
};
use super::*;
use crate::fs::session_files::load_session_exact;
use crate::handoff::{
    check_definition_hash, load_session_checkpoint, record_attempt_handoff,
    CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase, CriterionResult,
    HandoffOrigin, HandoffV2, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::monitor::events::CompletionEscalation;
use crate::orchestrator::monitor::MonitorEvent;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::transitions::{load_stage, update_stage};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub(super) struct Fixture {
    pub(super) _temp: TempDir,
    pub(super) work: PathBuf,
    pub(super) session: Session,
    pub(super) fingerprint: String,
    pub(super) orchestrator: Orchestrator,
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("scratch git command must start");
    assert!(output.status.success(), "scratch git command failed");
    String::from_utf8(output.stdout)
        .expect("git output must be UTF-8")
        .trim()
        .to_string()
}

fn init_repo(repo: &Path) -> String {
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "loom@example.invalid"]);
    git(repo, &["config", "user.name", "Loom Test"]);
    git(repo, &["config", "commit.gpgSign", "false"]);
    std::fs::write(repo.join("fixture.txt"), "fixture\n").unwrap();
    git(repo, &["add", "fixture.txt"]);
    git(repo, &["commit", "-q", "-m", "fixture"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn evidence(
    session: &Session,
    stage: &crate::models::stage::Stage,
    commit: &str,
    nonce: u32,
) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        commit: commit.to_string(),
        check_definition_hash: check_definition_hash(stage),
        exact_command: "cargo test --lib".into(),
        evidence_nonce: format!("evidence_nonce_{nonce:09}"),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "criterion-1".into(),
                passed: true,
            }],
            environment_policy: "stage-host-allowlist-v2".into(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("daemon_offline".into()),
        diagnostic_first_line: None,
        observed_at: "2026-09-14T10:00:00Z".into(),
        attestation: None,
    }
}

fn write_signal(work: &Path, session: &Session) {
    std::fs::create_dir_all(work.join("signals")).unwrap();
    std::fs::write(
        work.join("signals").join(format!("{}.md", session.id)),
        "signal",
    )
    .unwrap();
}

pub(super) fn fixture(repeats: u32) -> Fixture {
    let temp = handoff_work_dir();
    let commit = init_repo(temp.path());
    let work = temp.path().join(".loom/work");
    executing_stage(&work);
    let session = recorded_session(&work);
    update_stage("test-stage", &work, |stage| {
        stage.stage_type = StageType::Knowledge;
        stage.assign_session(session.id.clone());
        stage.begin_attempt(Utc::now() - chrono::Duration::seconds(5));
        Ok(())
    })
    .unwrap();
    let stage = load_stage("test-stage", &work).unwrap();
    for nonce in 1..=repeats {
        record_attempt_handoff(
            &session,
            &stage,
            &evidence(&session, &stage, &commit, nonce),
            &work,
        )
        .unwrap();
    }
    let fingerprint = load_session_checkpoint("test-stage", &session.id, &work)
        .unwrap()
        .unwrap()
        .blocker
        .unwrap()
        .fingerprint;
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".into(), session.clone());
    write_signal(&work, &session);
    Fixture {
        _temp: temp,
        work,
        session,
        fingerprint,
        orchestrator,
    }
}

pub(super) fn block(fixture: &mut Fixture) {
    fixture
        .orchestrator
        .handle_one_event(MonitorEvent::CompletionBlocked {
            stage_id: "test-stage".into(),
            session_id: fixture.session.id.clone(),
            fingerprint: fixture.fingerprint.clone(),
            repeat_count: 2,
            escalation: CompletionEscalation::Repeated,
        })
        .unwrap();
}

fn assert_parked(fixture: &Fixture) {
    let stage = load_stage("test-stage", &fixture.work).unwrap();
    let session = load_session_exact(&fixture.work, &fixture.session.id)
        .unwrap()
        .unwrap();
    let reason = stage.review_reason.unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(reason.contains(&fixture.fingerprint[..12]));
    assert!(reason.contains("2 verified attempts"));
    assert!(reason.contains("daemon_offline"));
    assert!(stage.attempt_started_at.is_none());
    assert!(stage.execution_secs.unwrap_or_default() >= 5);
    assert_eq!(session.status, SessionStatus::ContextExhausted);
    assert_eq!(
        session.exit_reason,
        Some(SessionExitReason::CriteriaBlocked)
    );
    assert!(!fixture
        .orchestrator
        .active_sessions
        .contains_key("test-stage"));
    assert!(!fixture
        .work
        .join("signals")
        .join(format!("{}.md", session.id))
        .exists());
    assert_eq!(
        fixture
            .orchestrator
            .graph
            .get_node("test-stage")
            .unwrap()
            .status,
        StageStatus::NeedsHumanReview
    );
    assert!(
        load_session_checkpoint("test-stage", &session.id, &fixture.work)
            .unwrap()
            .is_some()
    );
    assert!(fixture.orchestrator.graph.ready_stages().is_empty());
}

pub(super) fn assert_ownership_unknown(fixture: &Fixture) {
    let stage = load_stage("test-stage", &fixture.work).unwrap();
    let session = load_session_exact(&fixture.work, &fixture.session.id)
        .unwrap()
        .unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(stage
        .review_reason
        .unwrap()
        .contains("writer ownership unknown"));
    assert_eq!(session.status, SessionStatus::Running);
    assert_eq!(session.exit_reason, None);
    assert!(fixture
        .orchestrator
        .active_sessions
        .contains_key("test-stage"));
    assert!(fixture.orchestrator.graph.ready_stages().is_empty());
}

#[test]
fn first_failure_changes_nothing() {
    let mut fixture = fixture(1);
    fixture
        .orchestrator
        .handle_one_event(MonitorEvent::CompletionPending {
            stage_id: "test-stage".into(),
            session_id: fixture.session.id.clone(),
            fingerprint: fixture.fingerprint.clone(),
            repeat_count: 1,
        })
        .unwrap();

    assert_eq!(
        load_stage("test-stage", &fixture.work).unwrap().status,
        StageStatus::Executing
    );
    assert_eq!(
        load_session_exact(&fixture.work, &fixture.session.id)
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Running
    );
    assert!(fixture
        .orchestrator
        .active_sessions
        .contains_key("test-stage"));
    assert_eq!(
        fixture
            .orchestrator
            .graph
            .get_node("test-stage")
            .unwrap()
            .status,
        StageStatus::Executing
    );
}

#[test]
fn second_failure_with_writer_gone_parks() {
    let mut fixture = fixture(2);
    write_pid_file(&fixture.work, &fixture.session, Some(u64::MAX));
    block(&mut fixture);
    assert_parked(&fixture);
}

#[test]
fn alive_writer_is_killed_confirmed_and_parked() {
    let mut fixture = fixture(2);
    let pid = spawn_orphan_process();
    write_test_pid_identity(&fixture.work, &fixture.session, pid).unwrap();
    block(&mut fixture);
    assert!(!crate::process::is_process_alive(pid));
    assert_parked(&fixture);
}

#[test]
fn surviving_writer_escalates_without_retirement() {
    let mut fixture = fixture(2);
    write_pid_file(&fixture.work, &fixture.session, None);
    block(&mut fixture);
    assert_ownership_unknown(&fixture);
}

#[test]
fn missing_pid_escalates_without_retirement() {
    let mut fixture = fixture(2);
    block(&mut fixture);
    assert_ownership_unknown(&fixture);
}

#[test]
fn stale_session_or_non_executing_stage_changes_nothing() {
    let mut fixture = fixture(2);
    fixture
        .orchestrator
        .on_completion_blocker(
            "test-stage",
            "stale-session",
            &fixture.fingerprint,
            2,
            Some(CompletionEscalation::CapacityExhausted),
        )
        .unwrap();
    update_stage("test-stage", &fixture.work, |stage| {
        stage.force_status_with_reason(StageStatus::Blocked, "test stale guard");
        Ok(())
    })
    .unwrap();
    block(&mut fixture);

    assert_eq!(
        load_stage("test-stage", &fixture.work).unwrap().status,
        StageStatus::Blocked
    );
    assert_eq!(
        load_session_exact(&fixture.work, &fixture.session.id)
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Running
    );
    assert!(fixture
        .orchestrator
        .active_sessions
        .contains_key("test-stage"));
}

#[test]
fn forged_checkpoint_does_not_park() {
    let mut fixture = fixture(1);
    let stage = load_stage("test-stage", &fixture.work).unwrap();
    let commit = git(fixture._temp.path(), &["rev-parse", "HEAD"]);
    let mut checkpoint = CompletionCheckpoint::new(&stage.id, &fixture.session.id);
    checkpoint
        .record_attempt(&evidence(&fixture.session, &stage, &commit, 2))
        .unwrap();
    let fingerprint = checkpoint.blocker.as_ref().unwrap().fingerprint.clone();
    let handoff = HandoffV2::new(&fixture.session.id, &stage.id)
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint));
    std::fs::write(
        fixture.work.join("handoffs/test-stage-handoff-001.md"),
        format!("---\n{}---\n", handoff.to_yaml().unwrap()),
    )
    .unwrap();

    fixture
        .orchestrator
        .on_completion_blocker(
            "test-stage",
            &fixture.session.id,
            &fingerprint,
            2,
            Some(CompletionEscalation::Repeated),
        )
        .unwrap();

    assert_eq!(
        load_stage("test-stage", &fixture.work).unwrap().status,
        StageStatus::Executing
    );
}
