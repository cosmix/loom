use super::super::completion_evidence::{scratch_git, scratch_git_fixture, ScratchGitFixture};
use super::*;
use crate::daemon::protocol::{read_message, write_message};
use crate::fs::session_files::save_session;
use crate::handoff::{
    check_definition_hash, load_session_checkpoint, record_attempt_handoff,
    CompletionAttemptEvidence, CompletionPhase, CriterionResult, VerificationCheckpoint,
    COMPLETION_EVIDENCE_VERSION, STAGE_ENVIRONMENT_POLICY,
};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{AcceptanceCriterion, Stage, StageStatus, StageType};
use crate::verify::transitions::{load_stage, save_stage, update_stage};
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub(super) const STAGE: &str = "completion-proof";
pub(super) const EVIDENCE_NONCE: &str = "11111111111111111111111111111111";
pub(super) const COMPLETION_NONCE: &str = "22222222222222222222222222222222";

pub(super) struct Fixture {
    _scratch: ScratchGitFixture,
    repo: PathBuf,
    pub(super) work: PathBuf,
    stage: Stage,
    pub(super) session: Session,
    commit: String,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let scratch = scratch_git_fixture();
        let repo = scratch.repo.clone();
        let work = scratch.work.clone();
        let commit = scratch.commit.clone();
        let mut session = Session::new_knowledge(STAGE);
        session.status = SessionStatus::Running;
        let mut stage = Stage::new(STAGE.to_string(), None);
        stage.id = STAGE.to_string();
        stage.stage_type = StageType::Knowledge;
        stage.status = StageStatus::Executing;
        stage.session = Some(session.id.clone());
        stage.acceptance = vec![AcceptanceCriterion::Simple("true".to_string())];
        save_stage(&stage, &work).unwrap();
        save_session(&session, &work).unwrap();
        Self {
            _scratch: scratch,
            repo,
            work,
            stage,
            session,
            commit,
        }
    }

    pub(super) fn evidence(
        &self,
        nonce: &str,
        phase: CompletionPhase,
    ) -> CompletionAttemptEvidence {
        CompletionAttemptEvidence {
            version: COMPLETION_EVIDENCE_VERSION,
            stage_id: self.stage.id.clone(),
            session_id: self.session.id.clone(),
            commit: self.commit.clone(),
            check_definition_hash: check_definition_hash(&self.stage),
            exact_command: "loom check completion-proof".to_string(),
            evidence_nonce: nonce.to_string(),
            verification: VerificationCheckpoint {
                criteria: vec![CriterionResult {
                    id: "acceptance-0".to_string(),
                    passed: true,
                }],
                environment_policy: STAGE_ENVIRONMENT_POLICY.to_string(),
                environment: Vec::new(),
            },
            phase,
            external_failure_code: None,
            diagnostic_first_line: None,
            observed_at: "2026-09-14T10:00:00Z".to_string(),
            attestation: None,
        }
    }

    fn record_verified(&self) {
        let evidence = self.evidence(EVIDENCE_NONCE, CompletionPhase::VerifiedPendingAck);
        record_attempt_handoff(&self.session, &self.stage, &evidence, &self.work).unwrap();
    }
}

pub(super) fn dispatch_bytes_as(
    work: &Path,
    request: Request,
    token_authenticated: bool,
) -> Response {
    let mut bytes = Vec::new();
    write_message(&mut bytes, &request).unwrap();
    let decoded: Request = read_message(&mut Cursor::new(bytes)).unwrap();
    let auth = AuthorizedCompletion {
        _capability: Capability::User,
        credential_authenticated: token_authenticated,
        _peer_pid: Some(std::process::id()),
    };
    dispatch(&auth, decoded, work)
}

pub(super) fn dispatch_bytes(work: &Path, request: Request) -> Response {
    dispatch_bytes_as(work, request, true)
}

pub(super) fn record_request(f: &Fixture, evidence: CompletionAttemptEvidence) -> Request {
    Request::RecordCompletionEvidence {
        auth_token: "authenticated".to_string(),
        stage_id: STAGE.to_string(),
        session_id: f.session.id.clone(),
        evidence: Box::new(evidence),
    }
}

pub(super) fn complete_request(f: &Fixture, nonce: &str, evidence_nonce: &str) -> Request {
    Request::CompleteStage {
        auth_token: "authenticated".to_string(),
        stage_id: STAGE.to_string(),
        session_id: f.session.id.clone(),
        nonce: nonce.to_string(),
        evidence_nonce: evidence_nonce.to_string(),
    }
}

pub(super) fn assert_rejected_executing(f: &Fixture, response: Response) {
    assert!(
        matches!(response, Response::Error { .. }),
        "got {response:?}"
    );
    assert_eq!(
        load_stage(STAGE, &f.work).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn serialized_record_stores_checkpoint_without_completing_or_consuming_nonce() {
    let f = Fixture::new();
    let evidence = f.evidence(EVIDENCE_NONCE, CompletionPhase::VerifiedPendingAck);

    let response = dispatch_bytes(&f.work, record_request(&f, evidence.clone()));

    assert!(matches!(response, Response::Ok));
    assert_eq!(
        load_stage(STAGE, &f.work).unwrap().status,
        StageStatus::Executing
    );
    let checkpoint = load_session_checkpoint(STAGE, &f.session.id, &f.work)
        .unwrap()
        .unwrap();
    let mut latest = checkpoint.latest.unwrap();
    assert!(latest.attestation.take().is_some());
    assert_eq!(latest, evidence);
    assert!(checkpoint.accepted.is_none());
    assert!(!f.work.join("control-completions").exists());
}

#[test]
fn serialized_complete_accepts_matching_verified_checkpoint_and_receipts_it() {
    let f = Fixture::new();
    f.record_verified();

    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
    );

    assert!(matches!(response, Response::Ok));
    assert_eq!(
        load_stage(STAGE, &f.work).unwrap().status,
        StageStatus::Completed
    );
    let receipt = load_session_checkpoint(STAGE, &f.session.id, &f.work)
        .unwrap()
        .unwrap()
        .accepted
        .unwrap();
    assert_eq!(receipt.evidence_nonce, EVIDENCE_NONCE);
    assert_eq!(receipt.completion_nonce, COMPLETION_NONCE);
    assert_eq!(receipt.commit, f.commit);
}

#[test]
fn serialized_complete_rejects_a_missing_checkpoint() {
    let f = Fixture::new();
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
    );
    assert_rejected_executing(&f, response);
}

#[test]
fn serialized_complete_rejects_the_wrong_evidence_nonce() {
    let f = Fixture::new();
    f.record_verified();
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, "33333333333333333333333333333333"),
    );
    assert_rejected_executing(&f, response);
}

#[test]
fn serialized_complete_rejects_equal_completion_and_evidence_nonces() {
    let f = Fixture::new();
    f.record_verified();
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, EVIDENCE_NONCE, EVIDENCE_NONCE),
    );
    assert_rejected_executing(&f, response);
}

#[test]
fn serialized_complete_rejects_a_commit_moved_after_recording() {
    let f = Fixture::new();
    f.record_verified();
    let _ = scratch_git(
        &f.repo,
        &[
            "-c",
            "user.name=Loom Test",
            "-c",
            "user.email=loom@test.invalid",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "moved",
        ],
    );
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
    );
    assert_rejected_executing(&f, response);
}

#[test]
fn serialized_complete_rejects_a_changed_check_definition() {
    let f = Fixture::new();
    f.record_verified();
    update_stage(STAGE, &f.work, |stage| {
        stage
            .acceptance
            .push(AcceptanceCriterion::Simple("changed".to_string()));
        Ok(())
    })
    .unwrap();
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
    );
    assert_rejected_executing(&f, response);
}

#[test]
fn serialized_complete_rejects_a_latest_tool_failed_attempt() {
    let f = Fixture::new();
    f.record_verified();
    let failed_nonce = "44444444444444444444444444444444";
    let mut evidence = f.evidence(failed_nonce, CompletionPhase::ToolFailed);
    evidence.observed_at = "2026-09-14T10:01:00Z".to_string();
    assert!(matches!(
        dispatch_bytes(&f.work, record_request(&f, evidence)),
        Response::Ok
    ));
    let checkpoint = load_session_checkpoint(STAGE, &f.session.id, &f.work)
        .unwrap()
        .unwrap();
    assert_eq!(
        checkpoint.current_phase(),
        Some(CompletionPhase::ToolFailed)
    );
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, failed_nonce),
    );
    assert_rejected_executing(&f, response);
}

#[test]
fn serialized_complete_rejects_a_stale_stage_session_binding() {
    let f = Fixture::new();
    f.record_verified();
    update_stage(STAGE, &f.work, |stage| {
        stage.session = Some("another-session".to_string());
        Ok(())
    })
    .unwrap();
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
    );
    assert_rejected_executing(&f, response);
}

#[test]
fn serialized_record_rejects_evidence_for_another_stage() {
    let f = Fixture::new();
    let evidence = f.evidence(EVIDENCE_NONCE, CompletionPhase::VerifiedPendingAck);
    let mut request = record_request(&f, evidence);
    if let Request::RecordCompletionEvidence { stage_id, .. } = &mut request {
        *stage_id = "another-stage".to_string();
    }

    let response = dispatch_bytes(&f.work, request);

    assert_rejected_executing(&f, response);
    assert!(load_session_checkpoint(STAGE, &f.session.id, &f.work)
        .unwrap()
        .is_none());
}

#[test]
fn serialized_completion_nonce_replay_cannot_mutate_twice() {
    let f = Fixture::new();
    f.record_verified();
    let request = complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE);
    assert!(matches!(dispatch_bytes(&f.work, request), Response::Ok));
    let before_stage = serde_json::to_string(&load_stage(STAGE, &f.work).unwrap()).unwrap();
    let before_checkpoint = load_session_checkpoint(STAGE, &f.session.id, &f.work).unwrap();

    let replay = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
    );

    assert!(matches!(replay, Response::Error { .. }));
    assert_eq!(
        load_stage(STAGE, &f.work).unwrap().status,
        StageStatus::Completed
    );
    assert_eq!(
        serde_json::to_string(&load_stage(STAGE, &f.work).unwrap()).unwrap(),
        before_stage
    );
    assert_eq!(
        load_session_checkpoint(STAGE, &f.session.id, &f.work).unwrap(),
        before_checkpoint
    );
}

#[test]
fn non_completion_requests_are_refused() {
    let f = Fixture::new();
    let response = dispatch_bytes(
        &f.work,
        Request::Ping {
            auth_token: "authenticated".into(),
        },
    );
    assert!(matches!(response, Response::Error { .. }));
}
