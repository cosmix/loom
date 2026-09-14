use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::verify::transitions::save_stage;
use tempfile::TempDir;

const EVIDENCE_NONCE: &str = "fedcba9876543210fedcba9876543210";

fn active_pair(work_dir: &Path, stage_id: &str) -> (Stage, Session) {
    let mut session = Session::new();
    session.stage_id = Some(stage_id.to_string());
    session.status = SessionStatus::Running;
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    save_stage(&stage, work_dir).unwrap();
    save_session(&session, work_dir).unwrap();
    (stage, session)
}

fn stage_snapshot(work_dir: &Path, stage_id: &str) -> String {
    serde_json::to_string(&load_stage(stage_id, work_dir).unwrap()).unwrap()
}

#[test]
fn accepts_one_exact_active_completion_and_rejects_replay() {
    let fixture = super::super::completion_evidence::trusted_checkpoint_fixture(
        "build-api",
        StageType::Standard,
        SessionType::Stage,
        EVIDENCE_NONCE,
    );
    let nonce = "0123456789abcdef0123456789abcdef";

    assert!(matches!(
        fixture.complete(nonce, EVIDENCE_NONCE).unwrap(),
        Response::Ok
    ));
    assert_eq!(
        load_stage("build-api", &fixture.work).unwrap().status,
        StageStatus::Completed
    );
    assert!(fixture
        .complete(nonce, EVIDENCE_NONCE)
        .unwrap_err()
        .to_string()
        .contains("already consumed"));
}

#[test]
fn rejects_cross_stage_and_cross_session_without_mutation() {
    let temp = TempDir::new().unwrap();
    let (_, session) = active_pair(temp.path(), "build-api");
    let (_, other_session) = active_pair(temp.path(), "other-stage");
    let build_before = stage_snapshot(temp.path(), "build-api");
    let other_before = stage_snapshot(temp.path(), "other-stage");
    let cross_stage = handle_complete_stage(
        temp.path(),
        "other-stage",
        &session.id,
        "11111111111111111111111111111111",
        EVIDENCE_NONCE,
    );
    let cross_session = handle_complete_stage(
        temp.path(),
        "build-api",
        "session-other",
        "22222222222222222222222222222222",
        EVIDENCE_NONCE,
    );

    assert!(cross_stage.is_err());
    assert!(cross_session.is_err());
    assert_eq!(stage_snapshot(temp.path(), "build-api"), build_before);
    assert_eq!(stage_snapshot(temp.path(), "other-stage"), other_before);
    assert_eq!(other_session.status, SessionStatus::Running);
    assert!(!replay_path(temp.path(), "11111111111111111111111111111111").exists());
    assert!(!replay_path(temp.path(), "22222222222222222222222222222222").exists());
}

#[test]
fn rejects_preconsumed_nonce_without_mutating_active_stage() {
    let temp = TempDir::new().unwrap();
    let (_, session) = active_pair(temp.path(), "build-api");
    let before = stage_snapshot(temp.path(), "build-api");
    let nonce = "33333333333333333333333333333333";
    consume_nonce(temp.path(), nonce).unwrap();

    let error = handle_complete_stage(temp.path(), "build-api", &session.id, nonce, EVIDENCE_NONCE)
        .unwrap_err()
        .to_string();

    assert!(error.contains("already consumed"));
    assert_eq!(stage_snapshot(temp.path(), "build-api"), before);
}

#[test]
fn rejects_non_running_session_without_mutating_stage_or_consuming_nonce() {
    let temp = TempDir::new().unwrap();
    let (_, mut session) = active_pair(temp.path(), "build-api");
    session.status = SessionStatus::Completed;
    save_session(&session, temp.path()).unwrap();
    let before = stage_snapshot(temp.path(), "build-api");
    let nonce = "44444444444444444444444444444444";

    let error = handle_complete_stage(temp.path(), "build-api", &session.id, nonce, EVIDENCE_NONCE)
        .unwrap_err()
        .to_string();

    assert!(error.contains("active running stage session"));
    assert_eq!(stage_snapshot(temp.path(), "build-api"), before);
    assert!(!replay_path(temp.path(), nonce).exists());
}

fn knowledge_pair(work_dir: &Path, stage_id: &str, kind: SessionType) -> (Stage, Session) {
    let mut session = Session::new();
    session.session_type = kind;
    session.assign_to_stage(stage_id.to_string());
    session.status = SessionStatus::Running;
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.stage_type = StageType::Knowledge;
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    save_stage(&stage, work_dir).unwrap();
    save_session(&session, work_dir).unwrap();
    (stage, session)
}

#[test]
fn a_knowledge_stage_completes_merged_through_the_broker() {
    let fixture = super::super::completion_evidence::trusted_checkpoint_fixture(
        "notes",
        StageType::Knowledge,
        SessionType::Knowledge,
        EVIDENCE_NONCE,
    );
    let signals = fixture.work.join("signals");
    fs::write(
        signals.join(format!("{}.md", fixture.session.id)),
        "# Signal\n",
    )
    .unwrap();

    fixture
        .complete("55555555555555555555555555555555", EVIDENCE_NONCE)
        .unwrap();

    let stage = load_stage("notes", &fixture.work).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(
        stage.merged,
        "a knowledge stage has no branch and completes merged"
    );
    let record = fs::read_to_string(
        fixture
            .work
            .join("sessions")
            .join(format!("{}.md", fixture.session.id)),
    )
    .unwrap();
    let record: Session =
        crate::parser::frontmatter::parse_from_markdown(&record, "Session").unwrap();
    assert_eq!(record.status, SessionStatus::Completed);
    assert!(!signals.join(format!("{}.md", fixture.session.id)).exists());
}

#[test]
fn only_stage_and_knowledge_sessions_complete_through_the_broker() {
    let temp = TempDir::new().unwrap();
    let (_, session) = knowledge_pair(temp.path(), "notes", SessionType::Merge);
    let before = stage_snapshot(temp.path(), "notes");

    let error = handle_complete_stage(
        temp.path(),
        "notes",
        &session.id,
        "66666666666666666666666666666666",
        EVIDENCE_NONCE,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("active running stage session"), "{error}");
    assert_eq!(stage_snapshot(temp.path(), "notes"), before);
}

#[test]
fn completion_request_uses_user_capability_and_has_no_extensible_payload() {
    use crate::daemon::protocol::{Capability, Request};

    let request = Request::CompleteStage {
        auth_token: "secret".to_string(),
        stage_id: "build-api".to_string(),
        session_id: "session-123".to_string(),
        nonce: "0123456789abcdef0123456789abcdef".to_string(),
        evidence_nonce: EVIDENCE_NONCE.to_string(),
    };
    assert_eq!(request.required_capability(), Capability::User);
    let encoded = serde_json::to_string(&request).unwrap();
    for forbidden in [
        "command",
        "path",
        "no_verify",
        "force_unsafe",
        "assume_merged",
    ] {
        assert!(!encoded.contains(forbidden), "unexpected field: {encoded}");
    }
}
