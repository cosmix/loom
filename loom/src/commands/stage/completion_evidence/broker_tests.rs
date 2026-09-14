use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};
use tempfile::TempDir;

use super::broker::{run_broker, BrokerContext, BrokerOutcome, CompletionTransport};
use super::*;
use crate::daemon::Response;
use crate::fs::session_files::save_session;
use crate::handoff::{
    record_accepted_handoff, AcceptedReceipt, CompletionCheckpoint, CompletionPhase, HandoffOrigin,
    HandoffV2,
};
use crate::models::stage::{AcceptanceCriterion, StageStatus, StageType};
use crate::verify::transitions::save_stage;

pub(super) struct Fixture {
    _temp: TempDir,
    repo_root: PathBuf,
    pub(super) work_dir: PathBuf,
    pub(super) stage: Stage,
    pub(super) session: Session,
    commit: String,
}

impl Fixture {
    pub(super) fn new(status: StageStatus) -> Self {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir(&repo_root).unwrap();
        initialize_repository(&repo_root);
        let commit = command_output(&repo_root, &["rev-parse", "HEAD"]);
        let work_dir = repo_root.join(".loom").join("work");
        fs::create_dir_all(work_dir.join("handoffs")).unwrap();
        let mut session = Session::new();
        session.id = "broker-session".to_string();
        session.stage_id = Some("broker-stage".to_string());
        let mut stage = Stage::new("Broker stage".to_string(), None);
        stage.id = "broker-stage".to_string();
        stage.stage_type = StageType::Knowledge;
        stage.status = status;
        stage.session = Some(session.id.clone());
        stage.acceptance = vec![AcceptanceCriterion::Simple("true".to_string())];
        save_stage(&stage, &work_dir).unwrap();
        save_session(&session, &work_dir).unwrap();
        Self {
            _temp: temp,
            repo_root,
            work_dir,
            stage,
            session,
            commit,
        }
    }

    pub(super) fn output(&self) -> String {
        let command = pinned_command(&std::env::current_exe().unwrap(), &self.stage.id);
        format_evidence_record(&verified_evidence(
            &self.stage,
            &self.session.id,
            self.commit.clone(),
            command,
        ))
        .unwrap()
    }

    pub(super) fn run(
        &self,
        failed: bool,
        output: &str,
        transport: &FakeTransport,
    ) -> BrokerOutcome {
        let ctx = BrokerContext::new(
            &self.stage,
            &self.session,
            &self.work_dir,
            &self.repo_root,
            transport,
        );
        run_broker(ctx, failed, output)
    }
}

pub(super) enum CompleteResult {
    Response(Response),
    Error,
}

pub(super) struct FakeTransport<'a> {
    result: CompleteResult,
    record_fails: bool,
    pub(super) persist: bool,
    pub(super) receipt_on_error: bool,
    pub(super) forge_receipt_on_error: bool,
    fixture: &'a Fixture,
    records: RefCell<Vec<CompletionAttemptEvidence>>,
    completions: RefCell<Vec<(String, String)>>,
}

impl<'a> FakeTransport<'a> {
    pub(super) fn new(fixture: &'a Fixture, result: CompleteResult) -> Self {
        Self {
            result,
            record_fails: false,
            persist: false,
            receipt_on_error: false,
            forge_receipt_on_error: false,
            fixture,
            records: RefCell::new(Vec::new()),
            completions: RefCell::new(Vec::new()),
        }
    }

    fn write_forged_receipt(&self, completion_nonce: &str, evidence_nonce: &str) -> Result<()> {
        let Some(evidence) = self.records.borrow().last().cloned() else {
            bail!("missing recorded evidence");
        };
        let evidence = with_boundary_failure(
            &evidence,
            CompletionPhase::VerifiedPendingAck,
            "daemon_transport",
            None,
        );
        let mut checkpoint =
            CompletionCheckpoint::new(&self.fixture.stage.id, &self.fixture.session.id);
        checkpoint.record_attempt(&evidence)?;
        checkpoint.record_accepted(AcceptedReceipt {
            evidence_nonce: evidence_nonce.into(),
            completion_nonce: completion_nonce.into(),
            commit: self.fixture.commit.clone(),
            attestation: None,
        })?;
        let handoff = HandoffV2::new(&self.fixture.session.id, &self.fixture.stage.id)
            .with_origin(HandoffOrigin::CompletionEvidence)
            .with_completion_checkpoint(Some(checkpoint));
        fs::write(
            self.fixture
                .work_dir
                .join("handoffs/broker-stage-handoff-001.md"),
            format!("---\n{}---\n", handoff.to_yaml()?),
        )?;
        Ok(())
    }
}

impl CompletionTransport for FakeTransport<'_> {
    fn record(
        &self,
        session: &Session,
        stage: &Stage,
        evidence: &CompletionAttemptEvidence,
    ) -> Result<RecordRoute> {
        if self.record_fails {
            bail!("record unavailable");
        }
        self.records.borrow_mut().push(evidence.clone());
        if self.persist {
            record_host_fallback(session, stage, evidence.clone(), &self.fixture.work_dir)?;
        }
        Ok(RecordRoute::HostFallback)
    }

    fn complete(&self, completion_nonce: &str, evidence_nonce: &str) -> Result<Response> {
        self.completions
            .borrow_mut()
            .push((completion_nonce.to_string(), evidence_nonce.to_string()));
        if self.receipt_on_error {
            record_accepted_handoff(
                &self.fixture.session,
                &self.fixture.stage,
                AcceptedReceipt {
                    evidence_nonce: evidence_nonce.to_string(),
                    completion_nonce: completion_nonce.to_string(),
                    commit: self.fixture.commit.clone(),
                    attestation: None,
                },
                &self.fixture.work_dir,
            )?;
        }
        if self.forge_receipt_on_error {
            self.write_forged_receipt(completion_nonce, evidence_nonce)?;
        }
        match &self.result {
            CompleteResult::Response(response) => Ok(response.clone()),
            CompleteResult::Error => bail!("connection closed before acknowledgement"),
        }
    }
}

fn initialize_repository(path: &Path) {
    command(path, &["init", "-q"]);
    fs::write(path.join("seed"), "seed\n").unwrap();
    command(path, &["add", "seed"]);
    command(
        path,
        &[
            "-c",
            "user.name=Loom Test",
            "-c",
            "user.email=loom@example.invalid",
            "commit",
            "-qm",
            "seed",
        ],
    );
}

fn command(path: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(path)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn command_output(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn tool_failure_records_diagnostic_without_completion() {
    let fixture = Fixture::new(StageStatus::Executing);
    let transport = FakeTransport::new(&fixture, CompleteResult::Response(Response::Ok));

    let outcome = fixture.run(true, "\ncommand failed\nmore", &transport);

    assert_eq!(outcome, BrokerOutcome::ToolFailedRecorded);
    assert_eq!(
        transport.records.borrow()[0].phase,
        CompletionPhase::ToolFailed
    );
    assert!(transport.completions.borrow().is_empty());
}

#[test]
fn missing_record_never_completes() {
    let fixture = Fixture::new(StageStatus::Executing);
    let transport = FakeTransport::new(&fixture, CompleteResult::Response(Response::Ok));

    let outcome = fixture.run(false, "ordinary output", &transport);

    assert_eq!(outcome, BrokerOutcome::EvidenceMissingRecorded);
    assert_eq!(
        transport.records.borrow()[0].phase,
        CompletionPhase::EvidenceMissing
    );
    assert!(transport.completions.borrow().is_empty());
}

#[test]
fn wrong_commit_record_never_completes() {
    let fixture = Fixture::new(StageStatus::Executing);
    let transport = FakeTransport::new(&fixture, CompleteResult::Response(Response::Ok));
    let mut evidence = parse_evidence_record(&fixture.output()).unwrap();
    evidence.commit = "a".repeat(40);
    let output = format_evidence_record(&evidence).unwrap();

    let outcome = fixture.run(false, &output, &transport);

    assert_eq!(outcome, BrokerOutcome::EvidenceMissingRecorded);
    assert!(transport.completions.borrow().is_empty());
}

#[test]
fn valid_record_uses_distinct_nonces_and_is_accepted() {
    let fixture = Fixture::new(StageStatus::Executing);
    let transport = FakeTransport::new(&fixture, CompleteResult::Response(Response::Ok));

    let outcome = fixture.run(false, &fixture.output(), &transport);

    assert_eq!(outcome, BrokerOutcome::Accepted);
    let completion = &transport.completions.borrow()[0];
    assert_ne!(completion.0, completion.1);
    assert_eq!(transport.records.borrow()[0].evidence_nonce, completion.1);
}

#[test]
fn daemon_error_records_rejected_phase_with_same_evidence_nonce() {
    let fixture = Fixture::new(StageStatus::Executing);
    let transport = FakeTransport::new(
        &fixture,
        CompleteResult::Response(Response::Error {
            message: "not active".to_string(),
        }),
    );

    let outcome = fixture.run(false, &fixture.output(), &transport);

    assert_eq!(
        outcome,
        BrokerOutcome::DaemonRejected("not active".to_string())
    );
    let records = transport.records.borrow();
    assert_eq!(records[1].phase, CompletionPhase::DaemonRejected);
    assert_eq!(records[0].evidence_nonce, records[1].evidence_nonce);
}

#[test]
fn record_failure_does_not_complete() {
    let fixture = Fixture::new(StageStatus::Executing);
    let mut transport = FakeTransport::new(&fixture, CompleteResult::Response(Response::Ok));
    transport.record_fails = true;

    let outcome = fixture.run(false, &fixture.output(), &transport);

    assert_eq!(
        outcome,
        BrokerOutcome::EvidenceRecordFailed("record unavailable".to_string())
    );
    assert!(transport.completions.borrow().is_empty());
}

#[test]
fn outcome_lines_match_hook_contract() {
    let cases = [
        (BrokerOutcome::Accepted, "LOOM_CONTROL_OUTCOME accepted"),
        (
            BrokerOutcome::AcceptedReconciled,
            "LOOM_CONTROL_OUTCOME accepted_reconciled",
        ),
        (
            BrokerOutcome::ToolFailedRecorded,
            "LOOM_CONTROL_OUTCOME tool_failed_recorded",
        ),
        (
            BrokerOutcome::EvidenceMissingRecorded,
            "LOOM_CONTROL_OUTCOME evidence_missing_recorded",
        ),
        (
            BrokerOutcome::EvidenceRecordFailed("bad\nrecord".into()),
            "LOOM_CONTROL_OUTCOME evidence_record_failed badrecord",
        ),
        (
            BrokerOutcome::DaemonRejected("no".into()),
            "LOOM_CONTROL_OUTCOME daemon_rejected no",
        ),
        (
            BrokerOutcome::VerifiedPendingAck,
            "LOOM_CONTROL_OUTCOME verified_pending_ack",
        ),
        (
            BrokerOutcome::Uncertain("why".into()),
            "LOOM_CONTROL_OUTCOME uncertain why",
        ),
    ];

    for (outcome, expected) in cases {
        assert_eq!(outcome.outcome_line(), expected);
    }
}
