use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use serde_json::json;

use super::*;
use crate::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
};
use crate::subagent_lifecycle::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};

const STAGE: &str = "stage-a";
const LOOM_SESSION: &str = "loom-a";
const PARENT: &str = "parent-a";
const AGENT: &str = "agent-a";
const TOOL: &str = "tool-a";
const JOB: &str = "job-a";
const INVOCATION: &str = "invocation-a";

#[test]
fn receipt_activity_cannot_set_forwarder_state_without_lifecycle() {
    let fixture = Fixture::new("running", false);
    let summaries = fixture.gather();

    assert_eq!(summaries[0].state, SubagentState::ForwardUnknown);
}

#[test]
fn receipt_failure_cannot_set_forwarder_terminal_state() {
    let fixture = Fixture::new("failed", false);
    let summaries = fixture.gather();

    assert_eq!(summaries[0].state, SubagentState::ForwardUnknown);
    assert!(summaries[0].final_report.is_none());
}

#[test]
fn unknown_receipt_stays_forward_unknown() {
    let fixture = Fixture::new("running", true);
    let summaries = fixture.gather();

    assert_eq!(summaries[0].state, SubagentState::ForwardUnknown);
    assert!(summaries[0].final_report.is_none());
}

#[test]
fn empty_transcript_directory_keeps_expected_receipt_unsettled() {
    let fixture = Fixture::new("running", false);
    fs::remove_file(fixture.transcript()).unwrap();
    let summaries = fixture.gather();

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].state, SubagentState::ForwardWait);
    assert!(summaries[0].final_report.is_none());
}

#[test]
fn forwarder_json_carries_exact_receipt_backend_and_state() {
    let fixture = Fixture::new("running", false);
    let summary = fixture.gather().remove(0);
    let value = serde_json::to_value(summary).unwrap();

    assert_eq!(value["forward"]["receipt_id"], json!(fixture.receipt_id));
    assert_eq!(value["forward"]["backend_id"], json!(JOB));
    assert_eq!(value["forward"]["state"], json!("running"));
}

#[test]
fn non_forwarder_keeps_human_state_while_forwarding_use_drives_other_views() {
    let fixture = Fixture::new("running", false);
    fixture.set_agent_type("ordinary-worker");
    fixture.write_forwarding_transcript();
    fixture.write_claude_stop();
    let summaries = fixture.gather();
    let summary = &summaries[0];
    let value = serde_json::to_value(summary).unwrap();

    assert_eq!(summary.display_state, Some(SubagentState::Done));
    assert_eq!(summary.state, SubagentState::ForwardWait);
    assert_eq!(summary.final_report.as_deref(), Some("wrapper report"));
    assert_eq!(value["state"], json!("forward-wait"));
    assert_eq!(value["forward"]["receipt_id"], json!(fixture.receipt_id));
    assert_eq!(value["forward"]["backend_id"], json!(JOB));
    assert!(is_forward_state(summary.state));
}

#[test]
fn lifecycle_evidenced_forwarder_is_done_while_legacy_forwarder_is_unsettled() {
    let fixture = Fixture::new("running", false);
    let legacy = fixture.gather().remove(0);
    fixture.write_codex_success();
    let lifecycle = fixture.gather().remove(0);

    assert_eq!(
        (lifecycle.state, lifecycle.done_evidence),
        (SubagentState::Done, Some(classify::DoneEvidence::Lifecycle))
    );
    assert_eq!(legacy.state, SubagentState::ForwardUnknown);
}

#[test]
fn lifecycle_failure_cancellation_and_active_have_distinct_harvest_evidence() {
    let mut failed = super::super::summary::empty("failed-agent".into(), 0, None);
    failed.state = SubagentState::Failed;
    failed.terminal_reason = Some("backend failed".into());
    let mut cancelled = super::super::summary::empty("cancelled-agent".into(), 0, None);
    cancelled.state = SubagentState::Cancelled;
    cancelled.terminal_reason = Some("operator cancelled".into());
    let mut active = super::super::summary::empty("active-agent".into(), 0, None);
    active.state = SubagentState::Generating;

    assert_eq!(
        terminal_failure_evidence(&failed).as_deref(),
        Some("terminal failure evidence: agent=failed-agent state=failed reason=backend failed")
    );
    assert_eq!(
        terminal_failure_evidence(&cancelled).as_deref(),
        Some(
            "terminal failure evidence: agent=cancelled-agent state=cancelled reason=operator cancelled"
        )
    );
    assert!(terminal_failure_evidence(&active).is_none());
}

struct Fixture {
    _temp: tempfile::TempDir,
    work: PathBuf,
    state: PathBuf,
    transcripts: PathBuf,
    receipt_id: String,
}

impl Fixture {
    fn new(status: &str, foreign_locator: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let work = root.join("work");
        let state = root.join("state");
        let transcripts = root.join(PARENT).join("subagents");
        fs::create_dir_all(work.join("subagents").join(STAGE)).unwrap();
        fs::create_dir_all(work.join("stages")).unwrap();
        fs::create_dir_all(&transcripts).unwrap();
        fs::write(
            root.join(format!("{PARENT}.jsonl")),
            "{\"type\":\"parent\"}\n",
        )
        .unwrap();
        fs::write(
            work.join("stages/01-stage-a.md"),
            "---\nid: stage-a\nsession: loom-a\n---\n",
        )
        .unwrap();
        let mut fixture = Self {
            _temp: temp,
            work,
            state,
            transcripts,
            receipt_id: String::new(),
        };
        fixture.receipt_id = fixture.write_receipt(status, foreign_locator);
        fixture.set_agent_type("loom-codex-forwarder");
        fixture.write_transcript();
        fixture
    }

    fn transcript(&self) -> PathBuf {
        self.transcripts.join(format!("agent-{AGENT}.jsonl"))
    }

    fn write_receipt(&self, status: &str, foreign_locator: bool) -> String {
        let locator = self
            .state
            .join("workspace/jobs")
            .join(format!("{JOB}.json"));
        fs::create_dir_all(locator.parent().unwrap()).unwrap();
        fs::write(&locator, json!({"id": JOB, "status": status}).to_string()).unwrap();
        let identity = ForwardIdentity::new(PARENT, AGENT, TOOL, STAGE, LOOM_SESSION).unwrap();
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
            observed_at: Utc::now(),
            exit_code: None,
            codex_thread_id: None,
            locator: Some(
                if foreign_locator {
                    self._temp.path().join("foreign/job-a.json")
                } else {
                    locator
                }
                .display()
                .to_string(),
            ),
            model: None,
            effort: None,
        };
        fs::write(
            receipts_path(&self.work, STAGE).unwrap(),
            format!("{}\n", observation.encode_line().unwrap()),
        )
        .unwrap();
        observation.receipt_id
    }

    fn set_agent_type(&self, agent_type: &str) {
        let stage_dir = self.work.join("subagents").join(STAGE);
        let row = json!({
            "agent_id": AGENT,
            "agent_type": agent_type,
            "parent_session_id": PARENT,
            "stage_id": STAGE,
            "loom_session_id": LOOM_SESSION,
            "ts": "2026-09-13T10:00:00Z",
        });
        fs::write(stage_dir.join("starts.jsonl"), format!("{row}\n")).unwrap();
    }

    fn write_transcript(&self) {
        let entry = json!({
            "type": "assistant", "sessionId": PARENT,
            "timestamp": (Utc::now() - chrono::Duration::minutes(4)).to_rfc3339(),
            "message": {"content": [{"type": "text", "text": "wrapper report"}]},
        });
        fs::write(self.transcript(), entry.to_string() + "\n").unwrap();
    }

    fn write_claude_stop(&self) {
        let starts =
            crate::commands::subagents::ledger::StartedAgentTypeIndex::load(Some(&self.work));
        let agent_type = starts
            .resolve_exact(STAGE, PARENT, LOOM_SESSION, AGENT)
            .unwrap()
            .agent_type;
        let payload = json!({
            "session_id": PARENT,
            "agent_id": AGENT,
            "agent_type": agent_type,
            "transcript_path": self._temp.path().join(format!("{PARENT}.jsonl")),
            "agent_transcript_path": self.transcript(),
        });
        let environment = crate::subagent_lifecycle::ClaudeEnvironment {
            work_dir: &self.work,
            stage_id: STAGE,
            loom_session_id: LOOM_SESSION,
            observed_at: Utc::now(),
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
        )
        .unwrap();
        crate::subagent_lifecycle::store::append_locked(&self.work, &record).unwrap();
    }

    fn write_codex_success(&self) {
        let identity = WorkerIdentity::Codex {
            stage_id: STAGE.into(),
            loom_session_id: LOOM_SESSION.into(),
            parent_session_id: PARENT.into(),
            forwarder_agent_id: AGENT.into(),
            unit_id: "unit-a".into(),
            invocation_id: INVOCATION.into(),
            workspace_root: fs::canonicalize(self._temp.path()).unwrap(),
            execution: CodexExecution::Companion { job_id: JOB.into() },
        };
        for (state, kind) in [
            (LifecycleState::Running, CodexEvidenceKind::Authorization),
            (LifecycleState::Completed, CodexEvidenceKind::Observation),
        ] {
            let record = codex_record(&identity, state, kind);
            crate::subagent_lifecycle::store::append_locked(&self.work, &record).unwrap();
        }
    }

    fn write_forwarding_transcript(&self) {
        let tool_use = json!({
            "type": "assistant", "sessionId": PARENT, "timestamp": Utc::now().to_rfc3339(),
            "message": {"content": [{
                "type": "tool_use", "id": TOOL, "name": "Bash",
                "input": {"command": "~/.claude/hooks/loom/codex-forward.sh task 'x' --model gpt-5.6-terra --effort xhigh --write"}
            }]},
        });
        let tool_result = json!({
            "type": "user", "sessionId": PARENT, "timestamp": Utc::now().to_rfc3339(),
            "message": {"content": [{
                "type": "tool_result", "tool_use_id": TOOL, "content": ""
            }]},
        });
        let report = json!({
            "type": "assistant", "sessionId": PARENT,
            "timestamp": Utc::now().to_rfc3339(),
            "message": {"content": [{"type": "text", "text": "wrapper report"}]},
        });
        let transcript = [tool_use, tool_result, report]
            .into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(self.transcript(), transcript).unwrap();
    }

    fn gather(&self) -> Vec<SubagentSummary> {
        let index = forward_jobs::load_index_with_roots(
            &self.work,
            STAGE,
            Some(LOOM_SESSION),
            vec![self.state.clone()],
            Vec::new(),
        );
        let lifecycle =
            classify::lifecycle::Context::load(Some(&self.work), STAGE.into(), LOOM_SESSION.into());
        let Gathered::Found(summaries) = gather_with_index(
            &None,
            &Some(self.transcripts.clone()),
            classify::DEFAULT_DONE_DEBOUNCE_SECS,
            Some(&self.work),
            classify::resolve_subagent_ceiling(Some(&self.work)),
            Some(&lifecycle),
            Some(&index),
        ) else {
            panic!("explicit transcript directory must resolve");
        };
        summaries
    }
}

fn codex_record(
    identity: &WorkerIdentity,
    state: LifecycleState,
    evidence_kind: CodexEvidenceKind,
) -> LifecycleRecord {
    let terminal = state == LifecycleState::Completed;
    let evidence = CodexEvidence {
        evidence_kind,
        requested_model: "gpt-6-sol".into(),
        requested_effort: "xhigh".into(),
        invocation_id: INVOCATION.into(),
        job_id: Some(JOB.into()),
        thread_id: terminal.then(|| "thread-a".into()),
        turn_id: terminal.then(|| "turn-a".into()),
        tool_use_id: None,
        terminal_at: terminal.then(|| "2026-09-14T10:00:01.000Z".parse().unwrap()),
        outcome: if terminal {
            CodexEvidenceOutcome::Succeeded
        } else {
            CodexEvidenceOutcome::Running
        },
        detail: None,
    };
    let mut record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::CodexCompanion,
        identity: identity.clone(),
        observed_at: "2026-09-14T10:00:02.000Z".parse().unwrap(),
        state,
        evidence: serde_json::to_value(&evidence).unwrap(),
    };
    record.event_id = crate::subagent_lifecycle::store::codex_event_id(&record, &evidence).unwrap();
    record
}
