use super::model::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, WorkerOutcome, LIFECYCLE_VERSION,
};
use super::store::{append_locked, codex_event_id, replay, AppendOutcome};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const STAGE: &str = "stage-a";
const LOOM_SESSION: &str = "loom-session-a";
const PARENT: &str = "parent-uuid-a";

#[test]
fn shell_literal_jsonl_line_replays_as_success() -> Result<()> {
    let fixture = Fixture::new("shell-literal")?;
    let final_line = br#"{"type":"assistant","message":"done"}"#;
    fs::write(&fixture.worker, [final_line.as_slice(), b"\n"].concat())?;
    fixture.write_start("worker-a", "review")?;
    let evidence_digest = digest(final_line);
    let transcript_bytes = final_line.len() + 1;
    let event_id = stop_id(
        &fixture.worker,
        "worker-a",
        "review",
        transcript_bytes,
        &evidence_digest,
    )?;
    let line = format!(
        concat!(
            "{{\"version\":1,\"event_id\":\"{}\",",
            "\"producer\":\"claude_subagent_stop\",",
            "\"identity\":{{\"kind\":\"claude_subagent\",",
            "\"stage_id\":\"stage-a\",\"loom_session_id\":\"loom-session-a\",",
            "\"parent_session_id\":\"parent-uuid-a\",\"agent_id\":\"worker-a\",",
            "\"agent_type\":\"review\",\"transcript_path\":\"{}\"}},",
            "\"observed_at\":\"2026-09-13T10:00:01.000Z\",\"state\":\"completed\",",
            "\"evidence\":{{\"transcript_bytes\":{},",
            "\"final_record_sha256\":\"{}\"}}}}\n"
        ),
        event_id,
        fixture.worker.display(),
        transcript_bytes,
        evidence_digest
    );
    fixture.write_lifecycle(&line)?;

    let index = replay(&fixture.work)?;

    assert_eq!(
        index.claude_outcome(STAGE, LOOM_SESSION, PARENT, "worker-a", &fixture.worker),
        WorkerOutcome::Succeeded
    );
    Ok(())
}

#[test]
fn transcript_growth_invalidates_stop_terminality() -> Result<()> {
    let fixture = Fixture::new("growth")?;
    fs::write(&fixture.worker, "{\"message\":\"first\"}\n")?;
    fixture.write_start("worker-a", "review")?;
    let starts =
        crate::commands::subagents::ledger::StartedAgentTypeIndex::load(Some(&fixture.work));
    let payload = serde_json::json!({
        "session_id": PARENT,
        "agent_id": "worker-a",
        "agent_type": "review",
        "transcript_path": fixture.parent.display().to_string(),
        "agent_transcript_path": fixture.worker.display().to_string(),
    });
    let observed_at = "2026-09-13T10:00:01Z".parse::<DateTime<Utc>>()?;
    let env = super::ClaudeEnvironment {
        work_dir: &fixture.work,
        stage_id: STAGE,
        loom_session_id: LOOM_SESSION,
        observed_at,
    };
    let active = super::ActiveStageSession {
        stage_id: STAGE,
        loom_session_id: LOOM_SESSION,
    };
    let record = super::validate_subagent_stop(&payload, &env, &starts, &active)?;
    append_locked(&fixture.work, &record)?;
    fs::write(
        &fixture.worker,
        "{\"message\":\"first\"}\n{\"message\":\"later\"}\n",
    )?;

    let outcome = replay(&fixture.work)?.outcome(&record.identity);

    assert!(matches!(outcome, WorkerOutcome::Unknown(_)));
    Ok(())
}

#[test]
fn teammate_idle_remains_active() -> Result<()> {
    let fixture = Fixture::new("teammate-idle")?;
    let observed_at = "2026-09-13T10:00:01Z".parse::<DateTime<Utc>>()?;
    let env = super::ClaudeEnvironment {
        work_dir: &fixture.work,
        stage_id: STAGE,
        loom_session_id: LOOM_SESSION,
        observed_at,
    };
    let active = super::ActiveStageSession {
        stage_id: STAGE,
        loom_session_id: LOOM_SESSION,
    };
    let payload = serde_json::json!({
        "session_id": PARENT,
        "team_name": "team-a",
        "teammate_name": "reviewer-a",
        "transcript_path": fixture.parent.display().to_string(),
    });
    let record = super::validate_teammate_idle(&payload, &env, &active)?;
    append_locked(&fixture.work, &record)?;

    let outcome = replay(&fixture.work)?.outcome(&record.identity);

    assert_eq!(outcome, WorkerOutcome::Active);
    Ok(())
}

#[test]
fn parallel_forwarders_resolve_by_identity_despite_reversed_finish_order() -> Result<()> {
    let fixture = Fixture::new("parallel-codex")?;
    let mut auth_a = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", false)?;
    let mut auth_b = codex_record(&fixture.root, "forwarder-b", "unit-b", "job-b", false)?;
    let mut done_a = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", true)?;
    let mut done_b = codex_record(&fixture.root, "forwarder-b", "unit-b", "job-b", true)?;
    set_codex_id(&mut auth_a)?;
    set_codex_id(&mut auth_b)?;
    set_codex_id(&mut done_a)?;
    set_codex_id(&mut done_b)?;
    for record in [&auth_a, &auth_b, &done_b, &done_a] {
        assert_eq!(
            append_locked(&fixture.work, record)?,
            AppendOutcome::Appended
        );
    }

    let index = replay(&fixture.work)?;

    assert_eq!(
        (
            index.forwarded_outcome(STAGE, LOOM_SESSION, PARENT, "forwarder-a"),
            index.forwarded_outcome(STAGE, LOOM_SESSION, PARENT, "forwarder-b")
        ),
        (WorkerOutcome::Succeeded, WorkerOutcome::Succeeded)
    );
    Ok(())
}

#[test]
fn duplicate_id_with_different_content_is_conflicting() -> Result<()> {
    let fixture = Fixture::new("conflict")?;
    let mut authorization = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", false)?;
    set_codex_id(&mut authorization)?;
    assert_eq!(
        append_locked(&fixture.work, &authorization)?,
        AppendOutcome::Appended
    );
    assert_eq!(
        append_locked(&fixture.work, &authorization)?,
        AppendOutcome::Duplicate
    );
    let mut conflicting = authorization.clone();
    conflicting.observed_at = "2026-09-13T10:00:03Z".parse()?;
    assert_eq!(
        append_locked(&fixture.work, &conflicting)?,
        AppendOutcome::Conflict
    );

    let outcome = replay(&fixture.work)?.outcome(&authorization.identity);

    assert!(matches!(outcome, WorkerOutcome::Unknown(_)));
    Ok(())
}

#[test]
fn valid_unterminated_final_object_is_ignored() -> Result<()> {
    let fixture = Fixture::new("torn-final")?;
    let mut authorization = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", false)?;
    let mut terminal = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", true)?;
    set_codex_id(&mut authorization)?;
    set_codex_id(&mut terminal)?;
    let journal = format!(
        "{}\n{}",
        serde_json::to_string(&authorization)?,
        serde_json::to_string(&terminal)?
    );
    fixture.write_lifecycle(&journal)?;

    let outcome =
        replay(&fixture.work)?.forwarded_outcome(STAGE, LOOM_SESSION, PARENT, "forwarder-a");

    assert!(matches!(outcome, WorkerOutcome::Unknown(_)));
    Ok(())
}

#[test]
fn unknown_version_remains_unknown() -> Result<()> {
    let fixture = Fixture::new("unknown-version")?;
    let mut record = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", false)?;
    set_codex_id(&mut record)?;
    record.version = LIFECYCLE_VERSION + 1;
    append_locked(&fixture.work, &record)?;

    let outcome = replay(&fixture.work)?.outcome(&record.identity);

    assert_eq!(
        outcome,
        WorkerOutcome::Unknown("unknown lifecycle version 2".into())
    );
    Ok(())
}

fn codex_record(
    workspace: &Path,
    forwarder: &str,
    unit: &str,
    job: &str,
    terminal: bool,
) -> Result<LifecycleRecord> {
    let identity = WorkerIdentity::Codex {
        stage_id: STAGE.into(),
        loom_session_id: LOOM_SESSION.into(),
        parent_session_id: PARENT.into(),
        forwarder_agent_id: forwarder.into(),
        unit_id: unit.into(),
        invocation_id: format!("invocation-{unit}"),
        workspace_root: fs::canonicalize(workspace)?,
        execution: CodexExecution::Companion { job_id: job.into() },
    };
    let evidence = codex_evidence(unit, job, terminal)?;
    Ok(LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::CodexCompanion,
        identity,
        observed_at: "2026-09-13T10:00:02Z".parse()?,
        state: if terminal {
            LifecycleState::Completed
        } else {
            LifecycleState::Running
        },
        evidence: serde_json::to_value(evidence)?,
    })
}

fn codex_evidence(unit: &str, job: &str, terminal: bool) -> Result<CodexEvidence> {
    Ok(CodexEvidence {
        evidence_kind: if terminal {
            CodexEvidenceKind::Observation
        } else {
            CodexEvidenceKind::Authorization
        },
        requested_model: "gpt-5.6-sol".into(),
        requested_effort: "xhigh".into(),
        invocation_id: format!("invocation-{unit}"),
        job_id: Some(job.into()),
        thread_id: terminal.then(|| format!("thread-{unit}")),
        turn_id: terminal.then(|| format!("turn-{unit}")),
        tool_use_id: None,
        terminal_at: terminal
            .then(|| "2026-09-13T10:00:02Z".parse())
            .transpose()?,
        outcome: if terminal {
            CodexEvidenceOutcome::Succeeded
        } else {
            CodexEvidenceOutcome::Running
        },
        detail: None,
    })
}

fn set_codex_id(record: &mut LifecycleRecord) -> Result<()> {
    let evidence: CodexEvidence = serde_json::from_value(record.evidence.clone())?;
    record.event_id = codex_event_id(record, &evidence)?;
    Ok(())
}

fn stop_id(
    transcript: &Path,
    agent: &str,
    agent_type: &str,
    bytes: usize,
    final_digest: &str,
) -> Result<String> {
    let path = transcript.to_str().context("fixture path is not UTF-8")?;
    let byte_count = bytes.to_string();
    let fields = [
        STAGE,
        LOOM_SESSION,
        PARENT,
        agent,
        agent_type,
        path,
        &byte_count,
        final_digest,
    ];
    let mut canonical = b"loom.lifecycle.claude_subagent_stop.v1".to_vec();
    for field in fields {
        canonical.push(0);
        canonical.extend_from_slice(field.as_bytes());
    }
    Ok(digest(&canonical))
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    work: PathBuf,
    parent: PathBuf,
    worker: PathBuf,
}

impl Fixture {
    fn new(prefix: &str) -> Result<Self> {
        let temp = tempfile::Builder::new().prefix(prefix).tempdir()?;
        let root = temp.path().to_path_buf();
        let work = root.join("work");
        let project = root.join("claude-project");
        let parent = project.join(format!("{PARENT}.jsonl"));
        let worker = project
            .join(PARENT)
            .join("subagents")
            .join("agent-worker-a.jsonl");
        fs::create_dir_all(worker.parent().context("worker path has no parent")?)?;
        fs::create_dir_all(work.join("stages"))?;
        fs::write(&parent, "{\"type\":\"parent\"}\n")?;
        fs::write(
            work.join("stages/01-stage-a.md"),
            "---\nid: stage-a\nsession: loom-session-a\n---\n",
        )?;
        Ok(Self {
            _temp: temp,
            root,
            work,
            parent,
            worker,
        })
    }

    fn write_start(&self, agent: &str, agent_type: &str) -> Result<()> {
        let dir = self.work.join("subagents").join(STAGE);
        fs::create_dir_all(&dir)?;
        let row = serde_json::json!({
            "agent_id": agent,
            "agent_type": agent_type,
            "stage_id": STAGE,
            "parent_session_id": PARENT,
            "loom_session_id": LOOM_SESSION,
            "ts": "2026-09-13T10:00:00Z",
        });
        let row = format!("{row}\n");
        fs::write(dir.join("starts.jsonl"), row)?;
        Ok(())
    }

    fn write_lifecycle(&self, line: &str) -> Result<()> {
        let dir = self.work.join("subagents").join(STAGE);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("lifecycle.jsonl"), line)?;
        Ok(())
    }
}
