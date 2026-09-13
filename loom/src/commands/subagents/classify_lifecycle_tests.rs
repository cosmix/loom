use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::json;

use super::*;
use crate::commands::subagents::forward_jobs;
use crate::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
};
use crate::subagent_lifecycle::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};

const STAGE: &str = "stage-a";
const LOOM_SESSION: &str = "loom-session-a";
const PARENT: &str = "parent-a";
const AGENT: &str = "forwarder-a";
const TOOL: &str = "tool-a";
const JOB: &str = "job-a";
const INVOCATION: &str = "invocation-a";

#[test]
fn succeeded_lifecycle_beats_done_overlay() -> anyhow::Result<()> {
    let fixture = Fixture::new("completed")?;
    fixture.append_lifecycle(Some((LifecycleState::Completed, None)))?;

    let summary = fixture.analyze()?;
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(summary.done_evidence, Some(DoneEvidence::Lifecycle));
    assert!(summary.forward.is_none());
    Ok(())
}

#[test]
fn failed_lifecycle_beats_done_overlay() -> anyhow::Result<()> {
    let fixture = Fixture::new("completed")?;
    fixture.append_lifecycle(Some((LifecycleState::Failed, Some("backend failed"))))?;

    let summary = fixture.analyze()?;
    assert_eq!(summary.state, SubagentState::Failed);
    assert_eq!(summary.terminal_reason.as_deref(), Some("backend failed"));
    assert!(summary.forward.is_none());
    Ok(())
}

#[test]
fn cancelled_lifecycle_beats_done_overlay() -> anyhow::Result<()> {
    let fixture = Fixture::new("completed")?;
    fixture.append_lifecycle(Some((
        LifecycleState::Cancelled,
        Some("operator cancelled"),
    )))?;

    let summary = fixture.analyze()?;
    assert_eq!(summary.state, SubagentState::Cancelled);
    assert_eq!(
        summary.terminal_reason.as_deref(),
        Some("operator cancelled")
    );
    assert!(summary.forward.is_none());
    Ok(())
}

#[test]
fn active_lifecycle_beats_completed_receipt() -> anyhow::Result<()> {
    let fixture = Fixture::new("completed")?;
    fixture.append_lifecycle(None)?;

    let summary = fixture.analyze()?;
    assert_eq!(summary.state, SubagentState::ForwardWait);
    assert!(summary.forward.is_some());
    assert!(summary.done_evidence.is_none());
    assert!(summary.final_report.is_none());
    Ok(())
}

#[test]
fn unknown_lifecycle_beats_running_receipt() -> anyhow::Result<()> {
    let fixture = Fixture::new("running")?;
    fixture.append_lifecycle(Some((
        LifecycleState::Unknown,
        Some("lifecycle replay conflict"),
    )))?;

    let summary = fixture.analyze()?;
    assert_eq!(summary.state, SubagentState::ForwardUnknown);
    assert_eq!(
        summary.terminal_reason.as_deref(),
        Some("lifecycle replay conflict")
    );
    assert!(summary.done_evidence.is_none());
    assert!(summary.final_report.is_none());
    Ok(())
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    work: PathBuf,
    plugin_data: PathBuf,
    transcript: PathBuf,
}

impl Fixture {
    fn new(job_status: &str) -> anyhow::Result<Self> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().to_path_buf();
        let work = root.join(".loom/work");
        let plugin_data = root.join("plugin-data");
        let transcript = root
            .join(PARENT)
            .join("subagents")
            .join(format!("agent-{AGENT}.jsonl"));
        fs::create_dir_all(
            transcript
                .parent()
                .ok_or_else(|| anyhow::anyhow!("no parent"))?,
        )?;
        fs::create_dir_all(work.join("stages"))?;
        fs::create_dir_all(work.join("subagents").join(STAGE))?;
        fs::write(
            work.join("stages/01-stage-a.md"),
            "---\nid: stage-a\nsession: loom-session-a\n---\n",
        )?;
        write_transcript(&transcript)?;
        write_start(&work)?;
        write_forward_receipt(&work, &plugin_data, job_status)?;
        Ok(Self {
            _temp: temp,
            root,
            work,
            plugin_data,
            transcript,
        })
    }

    fn append_lifecycle(
        &self,
        terminal: Option<(LifecycleState, Option<&str>)>,
    ) -> anyhow::Result<()> {
        let identity = codex_identity(&self.root)?;
        let authorization = codex_record(
            &identity,
            LifecycleState::Running,
            None,
            CodexEvidenceKind::Authorization,
        )?;
        crate::subagent_lifecycle::store::append_locked(&self.work, &authorization)?;
        let (state, detail) = terminal.unwrap_or((LifecycleState::Running, None));
        let observation = codex_record(&identity, state, detail, CodexEvidenceKind::Observation)?;
        crate::subagent_lifecycle::store::append_locked(&self.work, &observation)?;
        Ok(())
    }

    fn analyze(&self) -> anyhow::Result<SubagentSummary> {
        let lifecycle =
            lifecycle::Context::load(Some(&self.work), STAGE.to_owned(), LOOM_SESSION.to_owned());
        let index = forward_jobs::load_index_with_roots(
            &self.work,
            STAGE,
            Some(LOOM_SESSION),
            vec![self.plugin_data.join("state")],
            Vec::new(),
        );
        analyze_with_evidence_at_ceiling(
            &self.transcript,
            AGENT.into(),
            0,
            Some(&self.work),
            u64::MAX,
            Some(&lifecycle),
            Some(&index),
        )
    }
}

fn write_transcript(path: &Path) -> anyhow::Result<()> {
    let row = json!({
        "type": "assistant",
        "sessionId": PARENT,
        "timestamp": "2026-09-14T10:00:00.000Z",
        "message": {"content": [{"type": "text", "text": "wrapper report"}]},
    });
    fs::write(path, format!("{row}\n"))?;
    Ok(())
}

fn write_start(work: &Path) -> anyhow::Result<()> {
    let row = json!({
        "agent_id": AGENT,
        "agent_type": "loom-codex-forwarder",
        "stage_id": STAGE,
        "parent_session_id": PARENT,
        "loom_session_id": LOOM_SESSION,
        "ts": "2026-09-14T10:00:00.000Z",
    });
    fs::write(
        work.join("subagents").join(STAGE).join("starts.jsonl"),
        format!("{row}\n"),
    )?;
    Ok(())
}

fn write_forward_receipt(work: &Path, plugin_data: &Path, status: &str) -> anyhow::Result<()> {
    let locator = plugin_data
        .join("state")
        .join("workspace")
        .join("jobs")
        .join(format!("{JOB}.json"));
    let jobs = locator
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no jobs parent"))?;
    fs::create_dir_all(jobs)?;
    let phase = (status == "completed").then_some("done");
    fs::write(
        &locator,
        json!({"id": JOB, "status": status, "phase": phase}).to_string(),
    )?;
    let identity = ForwardIdentity::new(PARENT, AGENT, TOOL, STAGE, LOOM_SESSION)?;
    let observation = ForwardObservation {
        schema: 1,
        receipt_id: identity.receipt_id(),
        parent_session_id: PARENT.into(),
        agent_id: AGENT.into(),
        tool_use_id: TOOL.into(),
        stage_id: STAGE.into(),
        loom_session_id: LOOM_SESSION.into(),
        backend: ForwardBackend::Companion,
        backend_id: JOB.into(),
        state: ForwardState::Queued,
        observed_at: "2026-09-14T10:00:00.000Z".parse()?,
        exit_code: None,
        codex_thread_id: None,
        locator: Some(locator.display().to_string()),
        model: None,
        effort: None,
    };
    fs::write(
        receipts_path(work, STAGE)?,
        format!("{}\n", observation.encode_line()?),
    )?;
    Ok(())
}

fn codex_identity(workspace: &Path) -> anyhow::Result<WorkerIdentity> {
    Ok(WorkerIdentity::Codex {
        stage_id: STAGE.into(),
        loom_session_id: LOOM_SESSION.into(),
        parent_session_id: PARENT.into(),
        forwarder_agent_id: AGENT.into(),
        unit_id: "unit-a".into(),
        invocation_id: INVOCATION.into(),
        workspace_root: fs::canonicalize(workspace)?,
        execution: CodexExecution::Companion { job_id: JOB.into() },
    })
}

fn codex_record(
    identity: &WorkerIdentity,
    state: LifecycleState,
    detail: Option<&str>,
    evidence_kind: CodexEvidenceKind,
) -> anyhow::Result<LifecycleRecord> {
    let terminal = state != LifecycleState::Running;
    let evidence = CodexEvidence {
        evidence_kind,
        requested_model: "gpt-5.6-sol".into(),
        requested_effort: "xhigh".into(),
        invocation_id: INVOCATION.into(),
        job_id: Some(JOB.into()),
        thread_id: terminal.then(|| "thread-a".into()),
        turn_id: terminal.then(|| "turn-a".into()),
        tool_use_id: None,
        terminal_at: terminal
            .then(|| "2026-09-14T10:00:01.000Z".parse())
            .transpose()?,
        outcome: outcome_for(state),
        detail: detail.map(str::to_owned),
    };
    lifecycle_record(identity, state, evidence)
}

fn lifecycle_record(
    identity: &WorkerIdentity,
    state: LifecycleState,
    evidence: CodexEvidence,
) -> anyhow::Result<LifecycleRecord> {
    let observed_at: DateTime<Utc> = "2026-09-14T10:00:02.000Z".parse()?;
    let mut record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::CodexCompanion,
        identity: identity.clone(),
        observed_at,
        state,
        evidence: serde_json::to_value(&evidence)?,
    };
    record.event_id = crate::subagent_lifecycle::store::codex_event_id(&record, &evidence)?;
    Ok(record)
}

fn outcome_for(state: LifecycleState) -> CodexEvidenceOutcome {
    match state {
        LifecycleState::Running => CodexEvidenceOutcome::Running,
        LifecycleState::Completed => CodexEvidenceOutcome::Succeeded,
        LifecycleState::Failed => CodexEvidenceOutcome::Failed,
        LifecycleState::Cancelled => CodexEvidenceOutcome::Cancelled,
        LifecycleState::Unknown | LifecycleState::TurnFinished | LifecycleState::Idle => {
            CodexEvidenceOutcome::Unknown
        }
    }
}
