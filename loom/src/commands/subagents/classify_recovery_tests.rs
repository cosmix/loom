//! Regression tests for two failure-recovery defects in `analyze`: a torn
//! multibyte UTF-8 tail must not fail the whole read (previously
//! `fs::read_to_string` errored on any invalid byte, dropping the agent from
//! `watch`'s settled check entirely), and an authoritative `Done` with no
//! text blocks in its last entry must not report an empty string as a
//! harvestable final report. Split out of `classify_tests.rs`, which is near
//! the file-size ceiling.

use super::*;
use crate::subagent_lifecycle::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity,
};

/// A transcript truncated mid-multibyte-character (a real possibility: the
/// file is being appended to while this reads it) must still yield the last
/// good entry rather than failing the whole read. Before the fix,
/// `fs::read_to_string` errored on the invalid byte and `analyze` returned
/// `Err`, which `render.rs`'s `gather` turns into a dropped agent -- and a
/// `watch` poll evaluated over the survivors could then report "every
/// subagent is done" while this one was silently missing.
#[test]
fn torn_multibyte_utf8_tail_does_not_fail_the_whole_read() {
    let temp = tempfile::tempdir().unwrap();
    let good = serde_json::json!({
        "type": "assistant",
        "timestamp": "2020-01-01T00:00:00.000Z",
        "message": {
            "role": "assistant",
            "content": [{"type": "text", "text": "finished before the torn byte"}],
        },
    })
    .to_string();
    let mut bytes = format!("{good}\n").into_bytes();
    // A lone continuation byte of a 3-byte UTF-8 sequence: invalid on its
    // own, appended with no closing bytes to simulate an in-progress write.
    bytes.extend_from_slice(&[0xE2, 0x82]);
    let path = temp.path().join("agent-x.jsonl");
    fs::write(&path, &bytes).unwrap();

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(
        summary.final_report.as_deref(),
        Some("finished before the torn byte")
    );
}

/// Authoritative lifecycle evidence can force `Done` even when the last
/// flushed transcript entry is a `tool_use` block with no text -- the
/// transcript may simply be lagging behind the hook. `final_report` must be
/// `None` in that case, not `Some("")`: `harvest` gates its print branch on
/// `final_report.is_some()`, so an empty-string report used to print a bare
/// header with no body and still count as "harvested".
#[test]
fn authoritative_done_with_no_text_blocks_has_no_final_report() -> anyhow::Result<()> {
    let fixture = LifecycleFixture::new("no-report", "review")?;
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": Utc::now().to_rfc3339(),
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "name": "Bash", "input": {}}],
            },
        })
    );
    fs::write(&fixture.worker, content)?;
    fixture.append_claude_stop()?;

    let summary = fixture.analyze(STAGE, LOOM_SESSION)?;
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(summary.done_evidence, Some(DoneEvidence::Lifecycle));
    assert!(
        summary.final_report.is_none(),
        "an authoritative Done with no text blocks must not report an empty string as harvestable"
    );
    Ok(())
}

const STAGE: &str = "stage-a";
const LOOM_SESSION: &str = "loom-session-a";
const PARENT: &str = "parent-uuid-a";
const AGENT: &str = "x";

#[test]
fn exact_lifecycle_success_bypasses_transcript_debounce() -> anyhow::Result<()> {
    let fixture = LifecycleFixture::new("exact-success", "review")?;
    fixture.append_claude_stop()?;

    let summary = fixture.analyze(STAGE, LOOM_SESSION)?;

    assert_eq!(
        (summary.state, summary.done_evidence),
        (SubagentState::Done, Some(DoneEvidence::Lifecycle))
    );
    Ok(())
}

#[test]
fn wrong_parent_stage_and_session_do_not_match() -> anyhow::Result<()> {
    let fixture = LifecycleFixture::new("wrong-identity", "review")?;
    fixture.append_claude_stop()?;
    let wrong_parent = fixture.worker_for("parent-uuid-b")?;

    let states = [
        fixture
            .analyze_path(&wrong_parent, STAGE, LOOM_SESSION)?
            .state,
        fixture.analyze("stage-b", LOOM_SESSION)?.state,
        fixture.analyze(STAGE, "loom-session-b")?.state,
    ];

    assert_eq!(states, [SubagentState::Generating; 3]);
    Ok(())
}

#[test]
fn forwarder_claude_stop_alone_does_not_settle() -> anyhow::Result<()> {
    let fixture = LifecycleFixture::new("forwarder-stop", "loom-codex-forwarder")?;
    fixture.append_claude_stop()?;

    let summary = fixture.analyze(STAGE, LOOM_SESSION)?;

    assert_eq!(
        (summary.state, summary.done_evidence),
        (SubagentState::ForwardUnknown, None)
    );
    Ok(())
}

#[test]
fn forwarded_failure_and_cancellation_keep_reasons() -> anyhow::Result<()> {
    let failed = LifecycleFixture::new("forward-failed", "loom-codex-forwarder")?;
    failed.append_codex_terminal(LifecycleState::Failed, "backend failed")?;
    let cancelled = LifecycleFixture::new("forward-cancelled", "loom-codex-forwarder")?;
    cancelled.append_codex_terminal(LifecycleState::Cancelled, "operator cancelled")?;

    let failed_summary = failed.analyze(STAGE, LOOM_SESSION)?;
    let cancelled_summary = cancelled.analyze(STAGE, LOOM_SESSION)?;

    assert_eq!(
        (
            failed_summary.state,
            failed_summary.terminal_reason.as_deref()
        ),
        (SubagentState::Failed, Some("backend failed"))
    );
    assert_eq!(
        (
            cancelled_summary.state,
            cancelled_summary.terminal_reason.as_deref()
        ),
        (SubagentState::Cancelled, Some("operator cancelled"))
    );
    Ok(())
}

struct LifecycleFixture {
    _temp: tempfile::TempDir,
    root: std::path::PathBuf,
    work: std::path::PathBuf,
    parent: std::path::PathBuf,
    worker: std::path::PathBuf,
}

impl LifecycleFixture {
    fn new(prefix: &str, agent_type: &str) -> anyhow::Result<Self> {
        let temp = tempfile::Builder::new().prefix(prefix).tempdir()?;
        let root = temp.path().to_path_buf();
        let work = root.join("work");
        let parent = root.join(format!("{PARENT}.jsonl"));
        let worker = root
            .join(PARENT)
            .join("subagents")
            .join(format!("agent-{AGENT}.jsonl"));
        let worker_parent = worker
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no worker parent"))?;
        fs::create_dir_all(worker_parent)?;
        fs::create_dir_all(work.join("stages"))?;
        fs::write(&parent, "{\"type\":\"parent\"}\n")?;
        fs::write(
            work.join("stages/01-stage-a.md"),
            "---\nid: stage-a\nsession: loom-session-a\n---\n",
        )?;
        let fixture = Self {
            _temp: temp,
            root,
            work,
            parent,
            worker,
        };
        fixture.write_start(agent_type)?;
        fixture.write_fresh_text(&fixture.worker)?;
        Ok(fixture)
    }

    fn write_start(&self, agent_type: &str) -> anyhow::Result<()> {
        let directory = self.work.join("subagents").join(STAGE);
        fs::create_dir_all(&directory)?;
        let row = serde_json::json!({
            "agent_id": AGENT,
            "agent_type": agent_type,
            "stage_id": STAGE,
            "parent_session_id": PARENT,
            "loom_session_id": LOOM_SESSION,
            "ts": "2026-09-13T10:00:00Z",
        });
        fs::write(directory.join("starts.jsonl"), format!("{row}\n"))?;
        Ok(())
    }

    fn write_fresh_text(&self, path: &Path) -> anyhow::Result<()> {
        let row = serde_json::json!({
            "type": "assistant",
            "timestamp": Utc::now().to_rfc3339(),
            "message": {"content": [{"type": "text", "text": "finished"}]},
        });
        fs::write(path, format!("{row}\n"))?;
        Ok(())
    }

    fn worker_for(&self, parent: &str) -> anyhow::Result<std::path::PathBuf> {
        let worker = self
            .root
            .join(parent)
            .join("subagents")
            .join(format!("agent-{AGENT}.jsonl"));
        let worker_parent = worker
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no worker parent"))?;
        fs::create_dir_all(worker_parent)?;
        self.write_fresh_text(&worker)?;
        Ok(worker)
    }

    fn append_claude_stop(&self) -> anyhow::Result<()> {
        let starts =
            crate::commands::subagents::ledger::StartedAgentTypeIndex::load(Some(&self.work));
        let agent_type = starts
            .resolve_exact(STAGE, PARENT, LOOM_SESSION, AGENT)
            .ok_or_else(|| anyhow::anyhow!("missing exact start"))?
            .agent_type;
        let payload = serde_json::json!({
            "session_id": PARENT,
            "agent_id": AGENT,
            "agent_type": agent_type,
            "transcript_path": self.parent.display().to_string(),
            "agent_transcript_path": self.worker.display().to_string(),
        });
        let observed_at = "2026-09-13T10:00:01Z".parse()?;
        let environment = crate::subagent_lifecycle::ClaudeEnvironment {
            work_dir: &self.work,
            stage_id: STAGE,
            loom_session_id: LOOM_SESSION,
            observed_at,
        };
        let active = crate::subagent_lifecycle::ActiveStageSession {
            stage_id: STAGE,
            loom_session_id: LOOM_SESSION,
        };
        let record = crate::subagent_lifecycle::validate_subagent_stop(
            &payload,
            &environment,
            &starts,
            &active,
        )?;
        crate::subagent_lifecycle::store::append_locked(&self.work, &record)?;
        Ok(())
    }

    fn append_codex_terminal(&self, state: LifecycleState, detail: &str) -> anyhow::Result<()> {
        let identity = WorkerIdentity::Codex {
            stage_id: STAGE.into(),
            loom_session_id: LOOM_SESSION.into(),
            parent_session_id: PARENT.into(),
            forwarder_agent_id: AGENT.into(),
            unit_id: "unit-a".into(),
            invocation_id: "invocation-a".into(),
            workspace_root: fs::canonicalize(&self.root)?,
            execution: CodexExecution::Companion {
                job_id: "job-a".into(),
            },
        };
        let authorization = codex_record(identity.clone(), LifecycleState::Running, None)?;
        let terminal = codex_record(identity, state, Some(detail))?;
        crate::subagent_lifecycle::store::append_locked(&self.work, &authorization)?;
        crate::subagent_lifecycle::store::append_locked(&self.work, &terminal)?;
        Ok(())
    }

    fn analyze(&self, stage: &str, session: &str) -> anyhow::Result<SubagentSummary> {
        self.analyze_path(&self.worker, stage, session)
    }

    fn analyze_path(
        &self,
        path: &Path,
        stage: &str,
        session: &str,
    ) -> anyhow::Result<SubagentSummary> {
        let lifecycle =
            lifecycle::Context::load(Some(&self.work), stage.to_owned(), session.to_owned());
        analyze_with_evidence_at_ceiling(
            path,
            AGENT.into(),
            DEFAULT_DONE_DEBOUNCE_SECS,
            Some(&self.work),
            u64::MAX,
            Some(&lifecycle),
            None,
        )
    }
}

fn codex_record(
    identity: WorkerIdentity,
    state: LifecycleState,
    detail: Option<&str>,
) -> anyhow::Result<LifecycleRecord> {
    let terminal = state != LifecycleState::Running;
    let evidence = CodexEvidence {
        evidence_kind: if terminal {
            CodexEvidenceKind::Observation
        } else {
            CodexEvidenceKind::Authorization
        },
        requested_model: "gpt-6-sol".into(),
        requested_effort: "xhigh".into(),
        invocation_id: "invocation-a".into(),
        job_id: Some("job-a".into()),
        thread_id: terminal.then(|| "thread-a".into()),
        turn_id: terminal.then(|| "turn-a".into()),
        tool_use_id: None,
        terminal_at: terminal
            .then(|| "2026-09-13T10:00:02Z".parse())
            .transpose()?,
        outcome: match state {
            LifecycleState::Running => CodexEvidenceOutcome::Running,
            LifecycleState::Failed => CodexEvidenceOutcome::Failed,
            LifecycleState::Cancelled => CodexEvidenceOutcome::Cancelled,
            _ => CodexEvidenceOutcome::Succeeded,
        },
        detail: detail.map(str::to_owned),
    };
    let mut record = LifecycleRecord {
        version: crate::subagent_lifecycle::LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::CodexCompanion,
        identity,
        observed_at: "2026-09-13T10:00:03Z".parse()?,
        state,
        evidence: serde_json::to_value(&evidence)?,
    };
    record.event_id = crate::subagent_lifecycle::store::codex_event_id(&record, &evidence)?;
    Ok(record)
}
