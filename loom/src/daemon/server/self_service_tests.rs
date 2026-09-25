//! Stage ownership and the self-service request policy of `self_service.rs`.

use super::*;
use crate::fs::session_files::save_session;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::save_stage;
use tempfile::TempDir;

/// The rule a socket dispute keeps: the stage session only.
fn session_owns_stage(work_dir: &Path, stage_id: &str, session_id: &str) -> Result<()> {
    session_owns_stage_as(work_dir, stage_id, session_id, DISPUTE_KINDS)
}

fn dispute(session_id: &str) -> Request {
    Request::DisputeCriteria {
        auth_token: "t".to_string(),
        stage_id: "build-api".to_string(),
        session_id: session_id.to_string(),
        criterion_index: 0,
        reason: "r".to_string(),
        evidence_commit: None,
        failure_output: None,
    }
}

fn file_dispute(session_id: &str) -> Request {
    Request::FileDispute {
        auth_token: "t".to_string(),
        stage_id: "build-api".to_string(),
        session_id: session_id.to_string(),
        kind: crate::models::dispute::DisputeKind::Contract {
            contract_id: "rejects-x".to_string(),
        },
        reason: "r".to_string(),
        evidence_commit: None,
    }
}

fn active_pair(work_dir: &Path, stage_id: &str) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = SessionStatus::Running;
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    save_stage(&stage, work_dir).unwrap();
    save_session(&session, work_dir).unwrap();
    session
}

fn observe(session_id: &str) -> Request {
    Request::ObserveChanges {
        auth_token: "t".to_string(),
        stage_id: "build-api".to_string(),
        session_id: session_id.to_string(),
    }
}

fn block(session_id: &str) -> Request {
    Request::BlockStage {
        auth_token: "t".to_string(),
        stage_id: "build-api".to_string(),
        session_id: session_id.to_string(),
        reason: "r".to_string(),
    }
}

fn evidence_request(session_id: &str) -> Request {
    use crate::handoff::{CompletionAttemptEvidence, CompletionPhase, VerificationCheckpoint};
    Request::RecordCompletionEvidence {
        auth_token: "t".to_string(),
        stage_id: "build-api".to_string(),
        session_id: session_id.to_string(),
        evidence: Box::new(CompletionAttemptEvidence {
            version: 1,
            stage_id: "build-api".to_string(),
            session_id: session_id.to_string(),
            commit: "a".repeat(40),
            check_definition_hash: "definition".to_string(),
            exact_command: "check".to_string(),
            evidence_nonce: "fedcba9876543210".to_string(),
            verification: VerificationCheckpoint::default(),
            phase: CompletionPhase::EvidenceMissing,
            external_failure_code: None,
            diagnostic_first_line: None,
            observed_at: "2026-09-14T00:00:00Z".to_string(),
            attestation: None,
        }),
    }
}

#[test]
fn only_the_own_stage_requests_can_be_authorized_by_the_connection() {
    assert_eq!(Some("s1"), self_service_session(&block("s1")));
    assert_eq!(Some("s0"), self_service_session(&evidence_request("s0")));
    assert_eq!(
        Some("s2"),
        self_service_session(&Request::CompleteStage {
            auth_token: "t".to_string(),
            stage_id: "build-api".to_string(),
            session_id: "s2".to_string(),
            nonce: "0123456789abcdef0123456789abcdef".to_string(),
            evidence_nonce: "fedcba9876543210fedcba9876543210".to_string(),
        })
    );
    assert_eq!(Some("s3"), self_service_session(&dispute("s3")));
    assert_eq!(Some("s5"), self_service_session(&file_dispute("s5")));
    assert_eq!(
        Some("s4"),
        self_service_session(&Request::FreezeContracts {
            auth_token: "t".to_string(),
            stage_id: "build-api".to_string(),
            session_id: "s4".to_string(),
            reports: Vec::new(),
        })
    );

    // The default. A User RPC that is not about the caller's own stage has
    // no session to name and must stay behind the token.
    assert_eq!(Some("s6"), self_service_session(&observe("s6")));
    assert_eq!(
        None,
        self_service_session(&Request::Ping {
            auth_token: "t".to_string()
        })
    );
    assert_eq!(
        None,
        self_service_session(&Request::SubscribeLogs {
            auth_token: "t".to_string()
        })
    );
}

#[test]
fn completion_ownership_is_left_to_its_own_locked_handler() {
    assert_eq!(
        None,
        ownership_to_enforce(&Request::CompleteStage {
            auth_token: "t".to_string(),
            stage_id: "build-api".to_string(),
            session_id: "s1".to_string(),
            nonce: "0123456789abcdef0123456789abcdef".to_string(),
            evidence_nonce: "fedcba9876543210fedcba9876543210".to_string(),
        })
    );
    assert_eq!(None, ownership_to_enforce(&evidence_request("s1")));
    assert_eq!(
        Some(("build-api", "s1", BLOCK_KINDS)),
        ownership_to_enforce(&block("s1"))
    );
    assert_eq!(
        Some(("build-api", "s1", DISPUTE_KINDS)),
        ownership_to_enforce(&dispute("s1"))
    );
    assert_eq!(
        Some(("build-api", "s1", DISPUTE_KINDS)),
        ownership_to_enforce(&file_dispute("s1"))
    );
    assert_eq!(
        Some(("build-api", "s1", OBSERVE_KINDS)),
        ownership_to_enforce(&observe("s1"))
    );
    // Nothing to prove when no session is named: the token carried it.
    assert_eq!(None, ownership_to_enforce(&block("")));
    assert_eq!(None, ownership_to_enforce(&observe("")));
}

#[test]
fn a_contract_session_may_block_its_stage_but_not_dispute() {
    let temp = TempDir::new().unwrap();
    let mut session = active_pair(temp.path(), "build-api");
    session.session_type = SessionType::Contract;
    save_session(&session, temp.path()).unwrap();

    for (request, admitted) in [
        (block(&session.id), true),
        (dispute(&session.id), false),
        (file_dispute(&session.id), false),
    ] {
        let (stage_id, session_id, kinds) = ownership_to_enforce(&request).unwrap();
        let owns = session_owns_stage_as(temp.path(), stage_id, session_id, kinds);
        assert_eq!(owns.is_ok(), admitted, "{request:?}");
    }
}

#[test]
fn a_live_session_owns_only_its_own_stage() {
    let temp = TempDir::new().unwrap();
    let mine = active_pair(temp.path(), "build-api");
    let theirs = active_pair(temp.path(), "other-stage");

    assert!(session_owns_stage(temp.path(), "build-api", &mine.id).is_ok());
    // The escalation this check exists to stop: a genuinely live session
    // naming somebody else's stage.
    assert!(session_owns_stage(temp.path(), "other-stage", &mine.id).is_err());
    assert!(session_owns_stage(temp.path(), "build-api", &theirs.id).is_err());
}

#[test]
fn a_session_that_is_no_longer_running_owns_nothing() {
    let temp = TempDir::new().unwrap();
    let mut session = active_pair(temp.path(), "build-api");
    session.status = SessionStatus::Completed;
    save_session(&session, temp.path()).unwrap();

    let error = session_owns_stage(temp.path(), "build-api", &session.id)
        .unwrap_err()
        .to_string();

    assert!(error.contains("active running stage session"), "{error}");
}

#[test]
fn a_knowledge_session_owns_its_stage_only_where_knowledge_is_admitted() {
    let temp = TempDir::new().unwrap();
    let mut session = active_pair(temp.path(), "notes");
    session.session_type = SessionType::Knowledge;
    save_session(&session, temp.path()).unwrap();

    assert!(session_owns_stage(temp.path(), "notes", &session.id).is_err());
    let both = [SessionType::Stage, SessionType::Knowledge];
    assert!(session_owns_stage_as(temp.path(), "notes", &session.id, &both).is_ok());
}

#[test]
fn ids_shaped_like_paths_are_refused_before_any_file_is_touched() {
    let temp = TempDir::new().unwrap();
    active_pair(temp.path(), "build-api");

    assert!(session_owns_stage(temp.path(), "../../etc/passwd", "s1").is_err());
    assert!(session_owns_stage(temp.path(), "build-api", "../../etc/passwd").is_err());
}

#[test]
fn an_unknown_session_owns_nothing() {
    let temp = TempDir::new().unwrap();
    active_pair(temp.path(), "build-api");

    assert!(session_owns_stage(temp.path(), "build-api", "session-nope").is_err());
}
