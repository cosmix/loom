//! Contracts for the agent-type sidecar Claude Code writes beside a subagent
//! transcript, and its rank below the hook ledger.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::agent_type_from_sidecar;
use crate::commands::usage::discovery::DiscoveredFile;
use crate::commands::usage::forward_join::ForwardJoin;
use crate::commands::usage::parse_all;
use crate::commands::usage::sections::agents;
use crate::commands::usage::time_range::TimeRange;
use crate::commands::usage::transcript_types::{Scope, Transcript};

const SESSION: &str = "s1";

/// `<root>/s1/subagents/agent-a1.jsonl`, one finished request.
fn write_transcript(root: &Path) -> PathBuf {
    let subagents = root.join(SESSION).join("subagents");
    fs::create_dir_all(&subagents).unwrap();
    let row = json!({
        "type": "assistant",
        "timestamp": "2026-08-30T00:01:00Z",
        "message": {
            "id": "m1",
            "model": "claude-sonnet-5",
            "content": [{"type": "text", "text": "done"}],
            "usage": {"input_tokens": 1}
        }
    });
    let path = subagents.join("agent-a1.jsonl");
    fs::write(&path, format!("{row}\n")).unwrap();
    path
}

fn parse_one(path: PathBuf, work_dir: Option<&Path>) -> Vec<Transcript> {
    let file = DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Subagent,
        session_id: SESSION.to_owned(),
        agent_id: Some("a1".to_owned()),
    };
    let range = TimeRange {
        since: Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        until: None,
    };
    parse_all(&[file], &range, work_dir, &ForwardJoin::load(None))
}

fn write_sidecar(transcript: &Path, agent_type: &str) {
    let sidecar = transcript.with_file_name("agent-a1.meta.json");
    fs::write(sidecar, json!({ "agentType": agent_type }).to_string()).unwrap();
}

#[test]
fn sidecar_fills_missing_agent_type() {
    let temp = tempfile::tempdir().unwrap();
    let transcript = write_transcript(temp.path());
    write_sidecar(&transcript, "loom-code-reviewer");

    let parsed = parse_one(transcript, None);

    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].agent_type.as_deref(), Some("loom-code-reviewer"));
    assert!(parsed[0].agent_type_from_sidecar);
    assert_eq!(parsed[0].stage_id, None);
    assert_eq!(parsed[0].loom_session_id, None);
    assert_eq!(parsed[0].requested_model, None);
    let report = agents::build(&parsed);
    assert_eq!(report.agent_type_ledger_matches, 0);
    assert_eq!(report.agent_type_sidecar_matches, 1);
}

#[test]
fn ledger_outranks_sidecar() {
    let temp = tempfile::tempdir().unwrap();
    let transcript = write_transcript(temp.path());
    write_sidecar(&transcript, "loom-code-reviewer");
    let work_dir = temp.path().join(".loom").join("work");
    let stage_dir = work_dir.join("subagents").join("stage-a");
    fs::create_dir_all(&stage_dir).unwrap();
    let start = json!({
        "agent_id": "a1",
        "agent_type": "loom-advisor",
        "parent_session_id": SESSION,
        "stage_id": "stage-a",
        "loom_session_id": "loom-1"
    });
    fs::write(stage_dir.join("starts.jsonl"), format!("{start}\n")).unwrap();

    let parsed = parse_one(transcript, Some(&work_dir));

    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].agent_type.as_deref(), Some("loom-advisor"));
    assert!(!parsed[0].agent_type_from_sidecar);
    assert_eq!(parsed[0].stage_id.as_deref(), Some("stage-a"));
    let report = agents::build(&parsed);
    assert_eq!(report.agent_type_ledger_matches, 1);
    assert_eq!(report.agent_type_sidecar_matches, 0);
}

#[test]
fn symlinked_sidecar_is_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let transcript = write_transcript(temp.path());
    let outside = temp.path().join("outside.json");
    fs::write(&outside, json!({"agentType": "loom-advisor"}).to_string()).unwrap();
    std::os::unix::fs::symlink(&outside, transcript.with_file_name("agent-a1.meta.json")).unwrap();

    assert_eq!(agent_type_from_sidecar(&transcript), None);
}
