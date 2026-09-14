//! The Claude stall rule table, driven by real transcripts whose last entry is
//! stamped in the past: `classify` derives idle time from that timestamp, so
//! aging the entry is the only way to exercise a budget without sleeping.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};
use tempfile::TempDir;

use super::*;
use crate::commands::subagents::wait::model::{WorkerKind, WorkerSpec};

const BUDGET: Duration = Duration::from_secs(600);

/// Write a one-entry transcript whose last entry is `idle_secs` old.
fn aged_transcript(temp: &TempDir, idle_secs: i64, entry_type: &str, content: Value) -> PathBuf {
    let stamped = Utc::now() - chrono::Duration::seconds(idle_secs);
    let line = json!({
        "type": entry_type,
        "timestamp": stamped.to_rfc3339_opts(SecondsFormat::Millis, true),
        "message": {"role": entry_type, "content": content},
    });
    let path = temp.path().join("agent-worker-a.jsonl");
    std::fs::write(&path, format!("{line}\n")).unwrap();
    path
}

fn text_turn(temp: &TempDir, idle_secs: i64) -> PathBuf {
    aged_transcript(
        temp,
        idle_secs,
        "assistant",
        json!([{"type": "text", "text": "the report"}]),
    )
}

fn tool_call(temp: &TempDir, idle_secs: i64, tool: &str) -> PathBuf {
    aged_transcript(
        temp,
        idle_secs,
        "assistant",
        json!([{"type": "tool_use", "name": tool, "input": {}}]),
    )
}

fn user_turn(temp: &TempDir, idle_secs: i64) -> PathBuf {
    aged_transcript(temp, idle_secs, "user", json!("keep going"))
}

fn reason(temp: &TempDir, path: &std::path::Path, budget: Duration) -> Option<String> {
    claude_stall(path, "worker-a", temp.path(), budget)
}

#[test]
fn done_turn_stalls_only_past_the_budget() {
    let temp = TempDir::new().unwrap();

    let fresh = text_turn(&temp, 300);
    assert_eq!(reason(&temp, &fresh, BUDGET), None);

    let stale = text_turn(&temp, 900);
    let detail = reason(&temp, &stale, BUDGET).unwrap();
    assert!(detail.contains("turn ended"), "{detail}");
    assert!(detail.contains("no SubagentStop record"), "{detail}");
    assert!(detail.contains("harvest its report"), "{detail}");
}

#[test]
fn generating_stalls_only_past_the_budget() {
    let temp = TempDir::new().unwrap();

    let fresh = user_turn(&temp, 300);
    assert_eq!(reason(&temp, &fresh, BUDGET), None);

    let stale = user_turn(&temp, 900);
    let detail = reason(&temp, &stale, BUDGET).unwrap();
    assert!(detail.contains("no transcript growth"), "{detail}");
    assert!(detail.contains("stall budget 600s"), "{detail}");
}

/// A nested spawn has no cap of its own, so it is never called hung at any idle
/// time -- the measured 1,425s tool call in `classify`'s module doc is exactly
/// this shape.
#[test]
fn nested_spawn_tool_wait_never_stalls() {
    let temp = TempDir::new().unwrap();

    for tool in ["Agent", "Task"] {
        let path = tool_call(&temp, 100_000, tool);
        assert_eq!(reason(&temp, &path, BUDGET), None, "{tool}");
    }
}

/// A Bash tool-wait is measured against its own floor, never the budget: 71 of
/// 19,007 sampled Bash calls ran past 600s and the longest reached 1,298s, so
/// anything under the 1,800s floor is a real call rather than a hung worker.
#[test]
fn bash_tool_wait_survives_every_measured_real_call() {
    let temp = TempDir::new().unwrap();

    for idle in [620, 900, 1_298, 1_800] {
        let path = tool_call(&temp, idle, "Bash");
        assert_eq!(reason(&temp, &path, BUDGET), None, "idle {idle}s");
    }

    let hung = tool_call(&temp, 2_000, "Bash");
    let detail = reason(&temp, &hung, BUDGET).unwrap();
    assert!(detail.contains("Bash call outstanding"), "{detail}");
    assert!(detail.contains("past the 1800s ceiling"), "{detail}");
}

/// A budget above the Bash floor raises the Bash threshold with it, so a
/// generous stage timeout never makes Bash detection stricter.
#[test]
fn bash_threshold_follows_a_larger_budget() {
    let temp = TempDir::new().unwrap();
    let path = tool_call(&temp, 2_000, "Bash");

    assert_eq!(reason(&temp, &path, Duration::from_secs(2_400)), None);
    assert!(reason(&temp, &path, BUDGET).is_some());
}

#[test]
fn ordinary_tool_wait_stalls_at_the_budget() {
    let temp = TempDir::new().unwrap();

    let fresh = tool_call(&temp, 300, "Read");
    assert_eq!(reason(&temp, &fresh, BUDGET), None);

    let stale = tool_call(&temp, 700, "Read");
    let detail = reason(&temp, &stale, BUDGET).unwrap();
    assert!(detail.contains("waiting on Read"), "{detail}");
    assert!(detail.contains("stall budget 600s"), "{detail}");
}

/// An unreadable transcript is not evidence that the agent stopped working.
#[test]
fn missing_transcript_is_not_a_stall() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("agent-worker-a.jsonl");

    assert_eq!(reason(&temp, &missing, BUDGET), None);
}

#[test]
fn detect_names_the_claude_worker_in_its_detail() {
    let temp = TempDir::new().unwrap();
    let transcript = text_turn(&temp, 900);
    let worker = BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Claude,
            id: "worker-a".into(),
        },
        lifecycle_identity: WorkerIdentity::ClaudeSubagent {
            stage_id: "stage-a".into(),
            loom_session_id: "loom-a".into(),
            parent_session_id: "parent-a".into(),
            agent_id: "worker-a".into(),
            agent_type: "loom-software-engineer".into(),
            transcript_path: transcript,
        },
        authority: None,
        evidence: Vec::new(),
    };

    let outcome = detect(&worker, temp.path(), BUDGET).unwrap();
    let WorkerOutcome::Stalled(detail) = outcome else {
        panic!("expected a stalled outcome, got {outcome:?}");
    };
    assert!(detail.starts_with("claude worker worker-a: "), "{detail}");
}

/// A teammate has no transcript of its own, and a direct Codex invocation is
/// killed by its own supervisor, so neither is measured here.
#[test]
fn identities_without_progress_evidence_are_left_active() {
    let temp = TempDir::new().unwrap();
    let worker = BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Claude,
            id: "mate-a".into(),
        },
        lifecycle_identity: WorkerIdentity::ClaudeTeammate {
            stage_id: "stage-a".into(),
            loom_session_id: "loom-a".into(),
            parent_session_id: "parent-a".into(),
            team_name: "team-a".into(),
            teammate_name: "mate-a".into(),
        },
        authority: None,
        evidence: Vec::new(),
    };

    assert!(detect(&worker, temp.path(), BUDGET).is_none());
}
