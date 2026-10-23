//! The `dispute_refused_freezes` tick step: which refusals it files, when,
//! and what it does once the freeze-refusal budget is spent.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serial_test::serial;
use tempfile::TempDir;

use super::super::tests::{handoff_work_dir, orchestrator_for, write_pid_file};
use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionBackendKind, SessionStatus};
use crate::orchestrator::coherence::live_worker_sessions;
use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat};
use crate::relay::ensure_dir_0700;
use crate::subagent_lifecycle::model::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};
use crate::subagent_lifecycle::store::{append_locked, codex_event_id};
use crate::verify::contracts::refusal::record;
use crate::verify::contracts::test_support::contract;
use crate::verify::transitions::{create_stage, load_stage, update_stage};

const STAGE: &str = "test-stage";

/// An auto-mode orchestrator over a v2 standard stage executing its
/// contract phase under the running writer `writer`, whose scratch
/// directory lives under `scratch_root`.
struct Fixture {
    _temp: TempDir,
    work: PathBuf,
    scratch_root: PathBuf,
    writer: Session,
    orchestrator: Orchestrator,
}

fn fixture() -> Fixture {
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let mut writer = Session::new_contract(STAGE);
    writer.status = SessionStatus::Running;
    writer.backend = SessionBackendKind::Tmux;
    save_session(&writer, &work).unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        session: Some(writer.id.clone()),
        working_dir: Some(".".to_string()),
        plan_version: 2,
        stage_type: StageType::Standard,
        contracts: vec![contract()],
        ..Stage::default()
    };
    create_stage(&stage, &work).unwrap();
    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing(STAGE).unwrap();
    let scratch_root = temp.path().join("scratch");
    // SAFETY: `getuid` has no preconditions and cannot fail.
    ensure_dir_0700(&scratch_root.join(&writer.id), unsafe { libc::getuid() }).unwrap();
    Fixture {
        _temp: temp,
        work,
        scratch_root,
        writer,
        orchestrator,
    }
}

impl Fixture {
    fn tick_at(&mut self, now: SystemTime) {
        let root = self.scratch_root.clone();
        self.orchestrator
            .dispute_refused_freezes_at(Some(&root), now);
    }

    fn tick(&mut self) {
        self.tick_at(SystemTime::now());
    }

    /// A tick once a refusal written now has been quiet long enough.
    fn tick_after_quiet(&mut self) {
        self.tick_at(SystemTime::now() + Duration::from_secs(REFUSAL_QUIET_SECS + 1));
    }

    fn record(&self, session_id: &str) {
        let problems = vec!["src/lib.rs is not a contract file".to_string()];
        record_daemon_refusal(
            &self.work,
            STAGE,
            session_id,
            &problems,
            RefusalSource::Daemon,
        )
        .unwrap();
    }

    fn refuse_locally(&self) {
        let problems = vec!["tests/data.json is not matched by a harness glob".to_string()];
        record(&self.scratch_root.join(&self.writer.id), &problems).unwrap();
    }

    fn stage(&self) -> Stage {
        load_stage(STAGE, &self.work).unwrap()
    }

    fn requests(&self) -> Vec<PathBuf> {
        let dir = self.work.join("disputes").join(STAGE);
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut requests: Vec<PathBuf> = entries
            .map(|entry| entry.unwrap().path().join("request.md"))
            .filter(|path| path.is_file())
            .collect();
        requests.sort();
        requests
    }

    fn assert_nothing_filed(&self) {
        assert_eq!(self.stage().status, StageStatus::Executing);
        assert!(self.requests().is_empty(), "{:?}", self.requests());
    }

    fn assert_filed_once(&self) -> String {
        assert_eq!(self.stage().status, StageStatus::NeedsAdjudication);
        let requests = self.requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert_eq!(daemon_refusal(&self.work, STAGE).unwrap(), None);
        std::fs::read_to_string(&requests[0]).unwrap()
    }
}

#[test]
#[serial]
fn a_daemon_record_is_filed_in_the_same_tick() {
    let mut fx = fixture();
    fx.record(&fx.writer.id);

    fx.tick();

    let request = fx.assert_filed_once();
    assert!(
        request.contains("src/lib.rs is not a contract file"),
        "{request}"
    );
    assert!(!request.contains(WRITER_PREFIX), "{request}");
    assert_eq!(fx.stage().tally.freeze_refusal_disputes, 1);
}

#[test]
#[serial]
fn a_record_for_another_session_is_dropped_and_nothing_is_filed() {
    let mut fx = fixture();
    fx.record("an-earlier-writer");

    fx.tick();

    fx.assert_nothing_filed();
    assert_eq!(daemon_refusal(&fx.work, STAGE).unwrap(), None);
}

#[test]
#[serial]
fn a_local_refusal_is_filed_only_once_it_has_been_quiet() {
    let mut fx = fixture();
    fx.refuse_locally();

    fx.tick();
    fx.assert_nothing_filed();
    assert_eq!(daemon_refusal(&fx.work, STAGE).unwrap(), None);

    fx.tick_after_quiet();
    let request = fx.assert_filed_once();
    assert!(
        request.contains("writer-reported: tests/data.json is not matched"),
        "{request}"
    );
}

#[test]
#[serial]
fn a_local_refusal_waits_while_the_writer_still_heartbeats() {
    let mut fx = fixture();
    fx.refuse_locally();
    let mut heartbeat = Heartbeat::new(STAGE.to_string(), fx.writer.id.clone());
    heartbeat.timestamp = chrono::Utc::now() + chrono::Duration::seconds(300);
    write_heartbeat(&fx.work, &heartbeat).unwrap();

    fx.tick_after_quiet();

    fx.assert_nothing_filed();
}

#[test]
#[serial]
fn a_local_refusal_waits_while_the_writer_runs_a_subagent() {
    let mut fx = fixture();
    fx.refuse_locally();
    active_child(&fx.work, fx._temp.path(), &fx.writer.id);

    fx.tick_after_quiet();

    fx.assert_nothing_filed();
}

#[test]
#[serial]
fn the_fourth_refusal_retires_the_writer_and_escalates() {
    let mut fx = fixture();
    update_stage(STAGE, &fx.work, |stage| {
        stage.tally.freeze_refusal_disputes = 3;
        Ok(())
    })
    .unwrap();
    // A start time no process could have: the retirement probes the writer dead.
    write_pid_file(&fx.work, &fx.writer, Some(u64::MAX));
    fx.record(&fx.writer.id);

    fx.tick();

    let stage = fx.stage();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.clone().unwrap_or_default();
    assert!(reason.contains("loom stage human-review"), "{reason}");
    assert!(
        reason.contains("src/lib.rs is not a contract file"),
        "{reason}"
    );
    assert!(
        live_worker_sessions(&fx.work, &stage).unwrap().is_empty(),
        "human-review --approve requires the writer gone"
    );
    assert!(fx.requests().is_empty(), "no dispute is pending");
    assert_eq!(daemon_refusal(&fx.work, STAGE).unwrap(), None);
}

#[test]
#[serial]
fn a_declined_filing_is_marked_and_never_retried() {
    let mut fx = fixture();
    fx.record(&fx.writer.id);

    settle_filing(
        &fx.work,
        STAGE,
        FreezeRefusalFiling::Declined("not admitted".to_string()),
    )
    .unwrap();
    fx.tick();

    fx.assert_nothing_filed();
    let record = daemon_refusal(&fx.work, STAGE).unwrap().unwrap();
    assert_eq!(record.declined.as_deref(), Some("not admitted"));
}

#[test]
#[serial]
fn a_tick_retried_after_a_failed_filing_writes_one_request() {
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let mut fx = fixture();
    fx.record(&fx.writer.id);
    // The request is written, then the stage update fails.
    let stages = fx.work.join("stages");
    std::fs::set_permissions(&stages, std::fs::Permissions::from_mode(0o555)).unwrap();
    fx.tick();
    std::fs::set_permissions(&stages, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(fx.stage().status, StageStatus::Executing);
    assert!(
        daemon_refusal(&fx.work, STAGE).unwrap().is_some(),
        "kept for the retry"
    );

    fx.tick();

    fx.assert_filed_once();
}

/// A codex child of `session_id` the lifecycle journal reports running.
fn active_child(work: &Path, workspace: &Path, session_id: &str) {
    for kind in [
        CodexEvidenceKind::Authorization,
        CodexEvidenceKind::Observation,
    ] {
        let evidence = CodexEvidence {
            evidence_kind: kind,
            requested_model: "gpt-6-sol".into(),
            requested_effort: "high".into(),
            invocation_id: "invocation-child".into(),
            job_id: Some("job-child".into()),
            thread_id: None,
            turn_id: None,
            tool_use_id: None,
            terminal_at: None,
            outcome: CodexEvidenceOutcome::Running,
            detail: None,
        };
        let mut record = LifecycleRecord {
            version: LIFECYCLE_VERSION,
            event_id: String::new(),
            producer: LifecycleProducer::CodexCompanion,
            identity: WorkerIdentity::Codex {
                stage_id: STAGE.into(),
                loom_session_id: session_id.into(),
                parent_session_id: "parent-session".into(),
                forwarder_agent_id: "forwarder-child".into(),
                unit_id: "unit-child".into(),
                invocation_id: "invocation-child".into(),
                workspace_root: std::fs::canonicalize(workspace).unwrap(),
                execution: CodexExecution::Companion {
                    job_id: "job-child".into(),
                },
            },
            observed_at: "2026-09-14T10:00:00Z".parse().unwrap(),
            state: LifecycleState::Running,
            evidence: serde_json::to_value(&evidence).unwrap(),
        };
        record.event_id = codex_event_id(&record, &evidence).unwrap();
        append_locked(work, &record).unwrap();
    }
    assert_eq!(
        replay(work)
            .unwrap()
            .session_child_disposition(STAGE, session_id),
        ChildDisposition::Active
    );
}
