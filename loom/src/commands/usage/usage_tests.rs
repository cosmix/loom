use std::fs;

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;
use crate::commands::usage::transcript::Scope;

fn write_start(stage_dir: &std::path::Path, parent_session_id: &str, agent_type: &str) {
    fs::create_dir_all(stage_dir).unwrap();
    fs::write(
        stage_dir.join("starts.jsonl"),
        format!(
            concat!(
                "not-json\n{{\"agent_id\":\"agent-1\",\"agent_type\":\"{agent_type}\",",
                "\"parent_session_id\":\"{parent_session_id}\",\"stage_id\":\"stage-a\",",
                "\"loom_session_id\":\"loom-1\"}}\n"
            ),
            agent_type = agent_type,
            parent_session_id = parent_session_id,
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

fn range() -> time_range::TimeRange {
    time_range::TimeRange {
        since: Utc.with_ymd_and_hms(2026, 8, 29, 0, 0, 0).single().unwrap(),
        until: None,
    }
}

fn empty_normalized(selection: ProviderSelection) -> provider::NormalizedProviderEvents {
    provider::normalize_provider_events(provider_input(selection))
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

    let parsed = parse_all(&files, &range(), Some(&work_dir));

    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed[0].agent_type.as_deref(),
        Some("loom-senior-software-engineer")
    );
    assert_eq!(parsed[0].stage_id.as_deref(), Some("stage-a"));
    assert_eq!(parsed[0].loom_session_id.as_deref(), Some("loom-1"));
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

    let parsed = parse_all(&files, &range(), Some(&temp.path().join("missing-work")));

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

    let parsed = parse_all(&files, &range(), Some(&work_dir));

    assert!(parsed[0].agent_type.is_none());
    assert_eq!(
        sections::agents::build(&parsed).agent_type_ledger_matches,
        0
    );
}

#[test]
fn legacy_json_keys_remain_with_versioned_provider_ledger() {
    let normalized = empty_normalized(ProviderSelection::Claude);
    let report = sections::build(
        &normalized.claude_transcripts,
        accounting::Windowing::FiveHour,
        normalized.ledger,
    );

    let value = serde_json::to_value(report).unwrap();

    for key in [
        "totals",
        "windows",
        "by_model",
        "lengths",
        "tools",
        "reads",
        "agents",
        "skills",
        "polling",
        "rewrites",
        "lifecycle",
        "edits",
    ] {
        assert!(value.get(key).is_some(), "missing legacy key {key}");
    }
    assert_eq!(value["provider_ledger"]["schema_version"], 1);
    assert_eq!(
        value["provider_ledger"]["providers"][0]["provider"],
        "claude"
    );
    assert!(value["provider_ledger"].get("quota_history").is_none());
}

#[test]
fn claude_provider_ledger_never_serializes_project_path_or_slug() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("nested/private.project");
    fs::create_dir_all(&project).unwrap();
    let path = project.join("session.jsonl");
    let absolute = project.to_string_lossy().into_owned();
    let line = json!({
        "type": "assistant",
        "cwd": absolute.clone(),
        "timestamp": "2026-08-30T00:01:00Z",
        "message": {"id": "m1", "model": "claude", "content": [], "usage": {"input_tokens": 1}}
    });
    fs::write(&path, format!("{line}\n")).unwrap();
    let slug = crate::commands::subagents::resolve::project_slug(&project);
    let transcript = transcript::parse(
        &discovery::DiscoveredFile {
            path,
            project_slug: slug.clone(),
            scope: Scope::Main,
            session_id: "session".to_owned(),
            agent_id: None,
        },
        &range(),
    )
    .unwrap();
    let normalized = provider::normalize_provider_events(provider::ProviderEventInput {
        claude_transcripts: vec![transcript],
        ..provider_input(ProviderSelection::Claude)
    });

    let serialized = serde_json::to_string(&normalized.ledger).unwrap();

    assert!(serialized.contains("\"project\":\"private.project\""));
    assert!(!serialized.contains(&slug));
    assert!(!serialized.contains(&absolute));
}

fn provider_input(selection: ProviderSelection) -> provider::ProviderEventInput<'static> {
    provider::ProviderEventInput {
        selection,
        range: range(),
        claude_transcripts: Vec::new(),
        claude_discovered_files: 0,
        claude_missing_roots: 0,
        codex_files: &[],
        codex_missing_roots: 0,
        codex_unreadable_directories: 0,
        receipts_root: None,
    }
}

#[test]
fn provider_selection_serializes_to_cli_names() {
    assert_eq!(
        serde_json::to_value(ProviderSelection::Claude).unwrap(),
        "claude"
    );
    assert_eq!(
        serde_json::to_value(ProviderSelection::Codex).unwrap(),
        "codex"
    );
    assert_eq!(serde_json::to_value(ProviderSelection::All).unwrap(), "all");
}
