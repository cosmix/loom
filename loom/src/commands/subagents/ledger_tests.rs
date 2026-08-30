//! Tests for hook-side ledger attribution rules.

use std::fs;

use super::*;

fn stage(work_dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = work_dir.path().join("subagents").join("stage-a");
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn starts_ledger_resolves_matching_agent_type() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        "not json\n{\"agent_id\":\"agent-x\",\"agent_type\":\"review\"}\n",
    )
    .unwrap();

    assert_eq!(
        agent_type(Some(work_dir.path()), "agent-x"),
        Some("review".into())
    );
}

#[test]
fn usage_lookup_prefers_the_matching_parent_session() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"old\",",
            "\"parent_session_id\":\"session-old\"}\n",
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"current\",",
            "\"parent_session_id\":\"session-current\"}\n"
        ),
    )
    .unwrap();

    assert_eq!(
        started_agent_type(Some(work_dir.path()), "agent-x", "session-current"),
        Some("current".into())
    );
}

#[test]
fn usage_lookup_does_not_fall_back_to_legacy_after_scoped_collision() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"legacy\"}\n",
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"other\",",
            "\"parent_session_id\":\"session-other\"}\n"
        ),
    )
    .unwrap();

    assert_eq!(
        started_agent_type(Some(work_dir.path()), "agent-x", "session-current"),
        None
    );
}

#[test]
fn usage_lookup_never_uses_unaddressable_spawn_rows() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("spawns.jsonl"),
        "{\"agent_type\":\"not-addressable-to-one-agent\"}\n",
    )
    .unwrap();

    assert_eq!(
        started_agent_type(Some(work_dir.path()), "agent-x", "session-current"),
        None
    );
}

#[test]
fn usage_lookup_rejects_ambiguous_loom_session_field() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"wrong\",",
            "\"session_id\":\"claude-parent\"}\n"
        ),
    )
    .unwrap();

    assert_eq!(
        started_agent_type(Some(work_dir.path()), "agent-x", "claude-parent"),
        None
    );
}

#[test]
fn usage_index_keeps_scoped_and_legacy_attribution_rules() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"scoped\",\"agent_type\":\"review\",",
            "\"parent_session_id\":\"parent-a\"}\n",
            "{\"agent_id\":\"scoped\",\"agent_type\":\"\",",
            "\"parent_session_id\":\"parent-a\"}\n",
            "{\"agent_id\":\"scoped\",\"agent_type\":\"implement\",",
            "\"parent_session_id\":\"parent-b\"}\n",
            "{\"agent_id\":\"legacy\",\"agent_type\":\"research\"}\n",
            "{\"agent_id\":\"legacy\",\"agent_type\":\"research\"}\n",
            "{\"agent_id\":\"ambiguous\",\"agent_type\":\"wrong\",",
            "\"session_id\":\"parent-a\"}\n",
            "{\"agent_id\":\"malformed\",\"agent_type\":\"legacy\"}\n",
            "{\"agent_id\":\"malformed\",\"agent_type\":\"\",",
            "\"parent_session_id\":null}\n"
        ),
    )
    .unwrap();

    let index = StartedAgentTypeIndex::load(Some(work_dir.path()));
    assert_eq!(index.get("scoped", "parent-a"), Some("review".into()));
    assert_eq!(index.get("scoped", "parent-c"), None);
    assert_eq!(index.get("legacy", "any-parent"), Some("research".into()));
    assert_eq!(index.get("ambiguous", "parent-a"), None);
    assert_eq!(index.get("malformed", "parent-a"), None);
}

#[test]
fn empty_agent_type_is_not_authoritative() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"\",",
            "\"parent_session_id\":\"claude-parent\"}\n"
        ),
    )
    .unwrap();

    assert_eq!(
        started_agent_type(Some(work_dir.path()), "agent-x", "claude-parent"),
        None
    );
    assert_eq!(agent_type(Some(work_dir.path()), "agent-x"), None);
}

#[test]
fn matching_spawns_rows_supply_the_safe_fallback() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("spawns.jsonl"),
        concat!(
            "{\"agent_type\":\"general-purpose\"}\n",
            "{\"agent_type\":\"general-purpose\"}\n"
        ),
    )
    .unwrap();

    assert_eq!(
        agent_type(Some(work_dir.path()), "agent-without-start"),
        Some("general-purpose".into())
    );
}

#[test]
fn conflicting_spawns_rows_remain_unknown() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("spawns.jsonl"),
        concat!(
            "{\"agent_type\":\"review\"}\n",
            "{\"agent_type\":\"implement\"}\n"
        ),
    )
    .unwrap();

    assert_eq!(
        agent_type(Some(work_dir.path()), "agent-without-start"),
        None
    );
}
