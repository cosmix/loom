//! Contracts for usage attribution without the hook ledgers: the stage comes
//! from a worktree project slug, and a subagent's requested model from the
//! parent transcript's spawn `tool_use`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};

use super::discovery::DiscoveredFile;
use super::transcript::Scope;
use super::{forward_join, parse_all, time_range};

fn range() -> time_range::TimeRange {
    time_range::TimeRange {
        since: Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        until: None,
    }
}

/// A finished transcript in the shape `usage_tests.rs` writes.
fn write_transcript(path: &Path, model: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            concat!(
                "{{\"type\":\"user\",\"timestamp\":\"2026-08-30T00:00:00Z\",",
                "\"message\":{{\"content\":\"Do the task.\"}}}}\n",
                "{{\"type\":\"assistant\",\"timestamp\":\"2026-08-30T00:01:00Z\",",
                "\"message\":{{\"id\":\"m1\",\"model\":\"{model}\",",
                "\"content\":[{{\"type\":\"text\",\"text\":\"done\"}}],",
                "\"usage\":{{\"input_tokens\":1}}}}}}\n"
            ),
            model = model,
        ),
    )
    .unwrap();
}

fn parse_one(file: DiscoveredFile) -> super::transcript::Transcript {
    let mut parsed = parse_all(
        &[file],
        &range(),
        None,
        &forward_join::ForwardJoin::load(None),
    );
    assert_eq!(parsed.len(), 1, "the transcript must parse");
    parsed.remove(0)
}

#[test]
fn stage_comes_from_worktree_project_slug_without_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("session-1.jsonl");
    write_transcript(&path, "claude-opus-5");

    let transcript = parse_one(DiscoveredFile {
        path,
        project_slug: "-home-u-src-app--worktrees-stage-b".to_owned(),
        scope: Scope::Main,
        session_id: "session-1".to_owned(),
        agent_id: None,
    });

    assert_eq!(transcript.stage_id.as_deref(), Some("stage-b"));
}

fn write_parent_with_spawn(project: &Path, session_id: &str) {
    let spawn = serde_json::json!({
        "type": "assistant",
        "timestamp": "2026-08-30T00:00:30Z",
        "message": {
            "id": "m0",
            "model": "claude-opus-5",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "Agent",
                "input": {"subagent_type": "loom-software-engineer", "model": "haiku"}
            }],
            "usage": {"input_tokens": 1}
        }
    });
    fs::write(
        project.join(format!("{session_id}.jsonl")),
        format!("{spawn}\n"),
    )
    .unwrap();
}

#[test]
fn requested_model_comes_from_parent_spawn_without_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    write_parent_with_spawn(project, "session-1");
    let subagents: PathBuf = project.join("session-1").join("subagents");
    let path = subagents.join("agent-1.jsonl");
    write_transcript(&path, "claude-haiku-5");
    fs::write(
        subagents.join("agent-1.meta.json"),
        r#"{"agentType":"loom-software-engineer","toolUseId":"toolu_1"}"#,
    )
    .unwrap();

    let transcript = parse_one(DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Subagent,
        session_id: "session-1".to_owned(),
        agent_id: Some("agent-1".to_owned()),
    });

    assert_eq!(transcript.requested_model.as_deref(), Some("haiku"));
}
