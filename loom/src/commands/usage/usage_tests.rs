use std::fs;

use chrono::{TimeZone, Utc};

use super::*;
use crate::commands::usage::transcript::Scope;

fn write_start(stage_dir: &std::path::Path, parent_session_id: &str, agent_type: &str) {
    fs::create_dir_all(stage_dir).unwrap();
    fs::write(
        stage_dir.join("starts.jsonl"),
        format!(
            "not-json\n{{\"agent_id\":\"agent-1\",\"agent_type\":\"{agent_type}\",\"parent_session_id\":\"{parent_session_id}\"}}\n"
        ),
    )
    .unwrap();
}

fn write_done_transcript(
    directory: &std::path::Path,
    prompt: &str,
    model: &str,
) -> std::path::PathBuf {
    let path = directory.join("agent-1.jsonl");
    fs::write(
        &path,
        format!(
            concat!(
                "{{\"type\":\"user\",\"timestamp\":\"2026-08-30T00:00:00Z\",",
                "\"message\":{{\"content\":\"{prompt}\"}}}}\n",
                "{{\"type\":\"assistant\",\"timestamp\":\"2026-08-30T00:01:00Z\",",
                "\"message\":{{\"id\":\"m1\",\"model\":\"{model}\",",
                "\"content\":[{{\"type\":\"text\",\"text\":\"done\"}}],",
                "\"usage\":{{\"input_tokens\":1}}}}}}\n"
            ),
            prompt = prompt,
            model = model,
        ),
    )
    .unwrap();
    path
}

fn subagent_file(path: std::path::PathBuf, session_id: &str) -> discovery::DiscoveredFile {
    discovery::DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Subagent,
        session_id: session_id.to_owned(),
        agent_id: Some("agent-1".to_owned()),
    }
}

/// Drive the production `parse_all` seam: metadata must travel from the
/// hook-written ledger into the parsed transcript before report building.
#[test]
fn parse_all_attaches_authoritative_agent_type_from_starts_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let stage_dir = work_dir.join("subagents/stage-a");
    write_start(&stage_dir, "session-1", "loom-senior-software-engineer");
    let files = vec![subagent_file(
        write_done_transcript(
            temp.path(),
            "Prompt quotes loom-software-engineer.",
            "claude-opus-5",
        ),
        "session-1",
    )];

    let parsed = parse_all(
        &files,
        Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        Some(&work_dir),
    );

    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed[0].agent_type.as_deref(),
        Some("loom-senior-software-engineer")
    );
    let report = sections::agents::build(&parsed);
    assert_eq!(report.agent_type_ledger_matches, 1);
    assert_eq!(
        report.by_agent_model[0].agent_type,
        "loom-senior-software-engineer"
    );
}

#[test]
fn parse_all_records_ledger_absence_without_disabling_prompt_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let transcript_path = temp.path().join("agent-1.jsonl");
    fs::write(
        &transcript_path,
        concat!(
            "{\"type\":\"user\",\"timestamp\":\"2026-08-30T00:00:00Z\",",
            "\"message\":{\"content\":\"Spawned as loom-software-engineer.\"}}\n",
            "{\"type\":\"assistant\",\"timestamp\":\"2026-08-30T00:01:00Z\",",
            "\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",",
            "\"content\":[{\"type\":\"text\",\"text\":\"done\"}],",
            "\"usage\":{\"input_tokens\":1}}}\n"
        ),
    )
    .unwrap();
    let files = vec![discovery::DiscoveredFile {
        path: transcript_path,
        project_slug: "project".to_owned(),
        scope: Scope::Subagent,
        session_id: "session-1".to_owned(),
        agent_id: Some("agent-1".to_owned()),
    }];

    let parsed = parse_all(
        &files,
        Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        Some(&temp.path().join("missing-work")),
    );

    assert_eq!(parsed.len(), 1);
    assert!(parsed[0].agent_type.is_none());
    let report = sections::agents::build(&parsed);
    assert_eq!(report.agent_type_ledger_matches, 0);
    assert_eq!(
        report.by_agent_model[0].agent_type,
        "loom-software-engineer"
    );
}

#[test]
fn parse_all_does_not_join_a_start_from_another_parent_session() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let stage_dir = work_dir.join("subagents/stage-a");
    fs::create_dir_all(&stage_dir).unwrap();
    fs::write(
        stage_dir.join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-1\",\"agent_type\":\"wrong\",",
            "\"parent_session_id\":\"older-session\"}\n"
        ),
    )
    .unwrap();

    let transcript_path = temp.path().join("agent-1.jsonl");
    fs::write(
        &transcript_path,
        concat!(
            "{\"type\":\"assistant\",\"timestamp\":\"2026-08-30T00:01:00Z\",",
            "\"message\":{\"model\":\"claude-sonnet-5\",\"content\":[],",
            "\"usage\":{\"input_tokens\":1}}}\n"
        ),
    )
    .unwrap();
    let files = vec![discovery::DiscoveredFile {
        path: transcript_path,
        project_slug: "project".to_owned(),
        scope: Scope::Subagent,
        session_id: "current-session".to_owned(),
        agent_id: Some("agent-1".to_owned()),
    }];

    let parsed = parse_all(
        &files,
        Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        Some(&work_dir),
    );

    assert!(parsed[0].agent_type.is_none());
    assert_eq!(
        sections::agents::build(&parsed).agent_type_ledger_matches,
        0
    );
}
