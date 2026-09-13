use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::json;

use super::*;
use crate::models::forward_receipt::{ForwardIdentity, ForwardObservation};

const STAGE: &str = "stage-a";
const LOOM_SESSION: &str = "loom-a";
const PARENT: &str = "parent-a";
const AGENT: &str = "agent-a";
const TOOL: &str = "tool-a";
const JOB: &str = "job-a";

#[test]
fn adapter_reads_fixtures_without_writing() {
    let fixture = Fixture::new("queued", None);
    let before = fixture.snapshot();

    let index = fixture.index();
    assert_eq!(
        overlay_for_agent(&index, AGENT, &fixture.transcript),
        Some(ForwardOverlay::ForwardWait)
    );

    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn deterministic_id_isolates_same_model_siblings() {
    let fixture = Fixture::new("queued", None);
    fixture.write_job("job-b", "failed", None);
    fixture.write_receipt(
        "agent-b",
        "tool-b",
        "job-b",
        Some(&fixture.locator("job-b")),
        ForwardBackend::Companion,
        "same-model",
    );
    fixture.write_transcript(AGENT, TOOL, &start_marker(JOB), true);

    let index = fixture.index();

    assert_eq!(
        overlay_for_agent(&index, AGENT, &fixture.transcript),
        Some(ForwardOverlay::ForwardWait)
    );
}

#[test]
fn foreign_companion_locator_is_unknown() {
    let fixture = Fixture::new("queued", Some("/tmp/foreign-job.json"));
    fixture.write_transcript(AGENT, TOOL, &start_marker(JOB), true);

    assert_eq!(
        overlay_for_agent(&fixture.index(), AGENT, &fixture.transcript),
        Some(ForwardOverlay::ForwardUnknown)
    );
}

#[test]
fn exact_companion_record_states_map_to_overlay() {
    for (status, phase, expected) in [
        ("queued", None, ForwardOverlay::ForwardWait),
        ("running", None, ForwardOverlay::ForwardWait),
        ("completed", Some("done"), ForwardOverlay::Done),
        ("failed", None, ForwardOverlay::ForwardFailed),
        ("cancelled", None, ForwardOverlay::ForwardFailed),
    ] {
        let fixture = Fixture::new(status, phase);
        fixture.write_transcript(AGENT, TOOL, &finished_marker(JOB, "succeeded", 0), true);
        assert_eq!(
            overlay_for_agent(&fixture.index(), AGENT, &fixture.transcript),
            Some(expected),
            "{status}"
        );
    }
}

#[test]
fn foreground_inline_marker_reconstructs_a_completed_direct_forward() {
    let fixture = Fixture::empty();
    fixture.write_transcript(
        AGENT,
        TOOL,
        &finished_direct_marker("thread-a", "succeeded", 0),
        true,
    );

    assert_eq!(
        overlay_for_agent(&fixture.index(), AGENT, &fixture.transcript),
        Some(ForwardOverlay::Done)
    );
}

#[test]
fn background_acknowledgement_reads_only_validated_task_output() {
    let fixture = Fixture::empty();
    let output = fixture.task_output("task-a");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, start_marker(JOB)).unwrap();
    fixture.write_background_transcript(TOOL, "task-a", &output, true);

    assert_eq!(
        overlay_for_agent(&fixture.index(), AGENT, &fixture.transcript),
        Some(ForwardOverlay::ForwardWait)
    );
}

#[test]
fn forwarding_use_without_receipt_or_marker_blocks_settlement() {
    let fixture = Fixture::empty();
    fixture.write_transcript(AGENT, TOOL, "", false);

    assert_eq!(
        overlay_for_agent(&fixture.index(), AGENT, &fixture.transcript),
        Some(ForwardOverlay::ForwardUnknown)
    );
}

#[test]
fn empty_transcript_does_not_settle_a_completed_expected_receipt() {
    let fixture = Fixture::new("completed", Some("done"));
    fs::write(&fixture.transcript, "").unwrap();

    assert_eq!(
        overlay_for_agent(&fixture.index(), AGENT, &fixture.transcript),
        Some(ForwardOverlay::ForwardWait)
    );
}

struct Fixture {
    _temp: tempfile::TempDir,
    work: PathBuf,
    state_root: PathBuf,
    task_root: PathBuf,
    transcript: PathBuf,
}

impl Fixture {
    fn empty() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let fixture = Self {
            work: root.join("work"),
            state_root: root.join("state"),
            task_root: root.join("task-output"),
            transcript: root
                .join(PARENT)
                .join("subagents")
                .join(format!("agent-{AGENT}.jsonl")),
            _temp: temp,
        };
        fs::create_dir_all(fixture.work.join("subagents").join(STAGE)).unwrap();
        fs::create_dir_all(fixture.transcript.parent().unwrap()).unwrap();
        fixture
    }

    fn new(status: &str, phase_or_locator: Option<&str>) -> Self {
        let fixture = Self::empty();
        let (phase, locator) = if phase_or_locator.is_some_and(|value| value.starts_with('/')) {
            (None, phase_or_locator.map(PathBuf::from))
        } else {
            (phase_or_locator, None)
        };
        fixture.write_job(JOB, status, phase);
        let default_locator = fixture.locator(JOB);
        fixture.write_receipt(
            AGENT,
            TOOL,
            JOB,
            locator.as_deref().or(Some(default_locator.as_path())),
            ForwardBackend::Companion,
            "same-model",
        );
        fixture
    }

    fn index(&self) -> ForwardIndex {
        load_index_with_roots(
            &self.work,
            STAGE,
            Some(LOOM_SESSION),
            vec![self.state_root.clone()],
            vec![self.task_root.clone()],
        )
    }

    fn locator(&self, job: &str) -> PathBuf {
        self.state_root
            .join("workspace")
            .join("jobs")
            .join(format!("{job}.json"))
    }

    fn write_job(&self, job: &str, status: &str, phase: Option<&str>) {
        let path = self.locator(job);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            json!({"id": job, "status": status, "phase": phase}).to_string(),
        )
        .unwrap();
    }

    fn write_receipt(
        &self,
        agent: &str,
        tool: &str,
        job: &str,
        locator: Option<&Path>,
        backend: ForwardBackend,
        model: &str,
    ) {
        let identity = ForwardIdentity::new(PARENT, agent, tool, STAGE, LOOM_SESSION).unwrap();
        let observation = ForwardObservation {
            schema: 1,
            receipt_id: identity.receipt_id(),
            parent_session_id: PARENT.into(),
            agent_id: agent.into(),
            tool_use_id: tool.into(),
            stage_id: STAGE.into(),
            loom_session_id: LOOM_SESSION.into(),
            backend,
            backend_id: job.into(),
            state: ForwardState::Queued,
            observed_at: Utc::now(),
            exit_code: None,
            codex_thread_id: None,
            locator: locator.map(|path| path.display().to_string()),
            model: Some(model.into()),
            effort: Some("high".into()),
        };
        let path = receipts_path(&self.work, STAGE).unwrap();
        let prior = fs::read_to_string(&path).unwrap_or_default();
        fs::write(
            path,
            format!("{prior}{}\n", observation.encode_line().unwrap()),
        )
        .unwrap();
    }

    fn write_transcript(&self, agent: &str, tool: &str, result: &str, done: bool) {
        assert_eq!(agent, AGENT);
        let mut rows = vec![tool_use(tool), tool_result(tool, result)];
        if done {
            rows.push(done_row());
        }
        write_rows(&self.transcript, rows);
    }

    fn write_background_transcript(&self, tool: &str, task: &str, output: &Path, done: bool) {
        let result = json!({"type":"user","sessionId":PARENT,"timestamp":Utc::now().to_rfc3339(),"message":{"content":[{"type":"tool_result","tool_use_id":tool,"content":"","toolUseResult":{"backgroundTaskId":task,"taskOutputPath":output}}]}});
        let mut rows = vec![tool_use(tool), result];
        if done {
            rows.push(done_row());
        }
        write_rows(&self.transcript, rows);
    }

    fn task_output(&self, task: &str) -> PathBuf {
        self.task_root
            .join("workspace")
            .join(PARENT)
            .join("tasks")
            .join(format!("{task}.output"))
    }

    fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
        snapshot_tree(self._temp.path())
    }
}

fn tool_use(id: &str) -> serde_json::Value {
    json!({"type":"assistant","sessionId":PARENT,"timestamp":Utc::now().to_rfc3339(),"message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":"~/.claude/hooks/loom/codex-forward.sh task 'x' --model gpt-5.6-terra --effort xhigh --write"}}]}})
}

fn tool_result(id: &str, text: &str) -> serde_json::Value {
    json!({"type":"user","sessionId":PARENT,"timestamp":Utc::now().to_rfc3339(),"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":text}]}})
}

fn done_row() -> serde_json::Value {
    json!({"type":"assistant","sessionId":PARENT,"timestamp":Utc::now().to_rfc3339(),"message":{"content":[{"type":"text","text":"done"}]}})
}

fn write_rows(path: &Path, rows: Vec<serde_json::Value>) {
    fs::write(
        path,
        rows.into_iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
}

fn start_marker(job: &str) -> String {
    format!("LOOM-FORWARD-START {{\"v\":1,\"backend\":\"companion\",\"job_id\":\"{job}\"}}")
}

fn finished_marker(job: &str, outcome: &str, code: i32) -> String {
    format!("{}\nLOOM-FORWARD-END {{\"v\":1,\"backend\":\"companion\",\"job_id\":\"{job}\",\"outcome\":\"{outcome}\",\"exit_code\":{code}}}\n--- LOOM-FORWARD-OUTPUT ---", start_marker(job))
}

fn finished_direct_marker(thread: &str, outcome: &str, code: i32) -> String {
    format!("LOOM-FORWARD-START {{\"v\":1,\"backend\":\"direct\",\"thread_id\":\"{thread}\"}}\nLOOM-FORWARD-END {{\"v\":1,\"backend\":\"direct\",\"thread_id\":\"{thread}\",\"outcome\":\"{outcome}\",\"exit_code\":{code}}}\n--- LOOM-FORWARD-OUTPUT ---")
}

fn snapshot_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut entries = fs::read_dir(root).unwrap().flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    entries
        .into_iter()
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                snapshot_tree(&path)
            } else {
                vec![(
                    path.strip_prefix(root).unwrap().display().to_string(),
                    fs::read(path).unwrap(),
                )]
            }
        })
        .collect()
}
