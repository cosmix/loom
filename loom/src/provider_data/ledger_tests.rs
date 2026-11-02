//! Tests for hook-side ledger attribution rules.

use std::fs;

use super::*;

fn stage(work_dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = work_dir.path().join("subagents").join("stage-a");
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn ledger_rejects_oversize_and_preserves_complete_rows_before_a_torn_line() {
    let work_dir = tempfile::tempdir().unwrap();
    let path = stage(&work_dir).join("starts.jsonl");
    let row = "{\"agent_id\":\"a\",\"agent_type\":\"review\"}\n";
    fs::write(&path, format!("{row}{{\"agent_id\":")).unwrap();
    assert_eq!(
        agent_type(Some(work_dir.path()), "a"),
        Some("review".into())
    );
    fs::write(&path, format!("{row}{}", " ".repeat(MAX_LEDGER_BYTES))).unwrap();
    assert_eq!(agent_type(Some(work_dir.path()), "a"), None);
}

#[test]
fn ledger_refuses_fifo_and_intermediate_symlink() {
    use std::os::unix::ffi::OsStrExt;
    let work_dir = tempfile::tempdir().unwrap();
    let path = stage(&work_dir).join("starts.jsonl");
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated pathname.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    assert!(json_lines(&path).is_empty());
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("starts.jsonl"), "{}\n").unwrap();
    let link = work_dir.path().join("subagents/link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    assert!(json_lines(&link.join("starts.jsonl")).is_empty());
}

/// A linked worktree reaches the work directory through its `.loom/work` symlink.
#[test]
fn ledger_reads_through_a_symlinked_work_directory_and_refuses_below_it() {
    let work_dir = tempfile::tempdir().unwrap();
    let row = "{\"agent_id\":\"a\",\"agent_type\":\"review\"}\n";
    fs::write(stage(&work_dir).join("starts.jsonl"), row).unwrap();
    let checkout = tempfile::tempdir().unwrap();
    let alias = checkout.path().join("work");
    std::os::unix::fs::symlink(work_dir.path(), &alias).unwrap();

    assert_eq!(agent_type(Some(&alias), "a"), Some("review".into()));

    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("starts.jsonl"), row).unwrap();
    let link = work_dir.path().join("subagents/link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    assert!(json_lines(&alias.join("subagents/link/starts.jsonl")).is_empty());
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
    assert!(index.get_metadata("legacy", "any-parent").is_none());
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

#[test]
fn usage_metadata_carries_authoritative_stage_and_loom_session() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"review\",",
            "\"parent_session_id\":\"parent-a\",\"stage_id\":\"stage-a\",",
            "\"loom_session_id\":\"loom-a\"}\n"
        ),
    )
    .unwrap();

    let metadata = StartedAgentTypeIndex::load(Some(work_dir.path()))
        .get_metadata("agent-x", "parent-a")
        .unwrap();

    assert_eq!(metadata.agent_type, "review");
    assert_eq!(metadata.stage_id.as_deref(), Some("stage-a"));
    assert_eq!(metadata.loom_session_id.as_deref(), Some("loom-a"));
}

#[test]
fn conflicting_stage_metadata_makes_scoped_join_unknown() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"review\",",
            "\"parent_session_id\":\"parent-a\",\"stage_id\":\"stage-a\"}\n",
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"review\",",
            "\"parent_session_id\":\"parent-a\",\"stage_id\":\"stage-b\"}\n"
        ),
    )
    .unwrap();

    let index = StartedAgentTypeIndex::load(Some(work_dir.path()));

    assert!(index.get_metadata("agent-x", "parent-a").is_none());
    assert!(index.get("agent-x", "parent-a").is_none());
}

#[test]
fn exact_lookup_requires_all_four_identity_fields() {
    let work_dir = tempfile::tempdir().unwrap();
    fs::write(
        stage(&work_dir).join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"review\",",
            "\"parent_session_id\":\"parent-a\",\"stage_id\":\"stage-a\",",
            "\"loom_session_id\":\"loom-a\",\"ts\":\"2026-09-13T10:00:00Z\"}\n",
            "{\"agent_id\":\"legacy\",\"agent_type\":\"review\",",
            "\"parent_session_id\":\"parent-a\"}\n",
            "{\"agent_id\":\"ambiguous\",\"agent_type\":\"review\",",
            "\"session_id\":\"parent-a\",\"stage_id\":\"stage-a\",",
            "\"loom_session_id\":\"loom-a\"}\n"
        ),
    )
    .unwrap();

    let index = StartedAgentTypeIndex::load(Some(work_dir.path()));
    let exact = index.resolve_exact("stage-a", "parent-a", "loom-a", "agent-x");

    assert_eq!(exact.map(|row| row.agent_type), Some("review".into()));
    assert!(index
        .resolve_exact("stage-b", "parent-a", "loom-a", "agent-x")
        .is_none());
    assert!(index
        .resolve_exact("stage-a", "parent-b", "loom-a", "agent-x")
        .is_none());
    assert!(index
        .resolve_exact("stage-a", "parent-a", "loom-b", "agent-x")
        .is_none());
    assert!(index
        .resolve_exact("stage-a", "parent-a", "loom-a", "agent-y")
        .is_none());
    assert!(index
        .resolve_exact("stage-a", "parent-a", "loom-a", "legacy")
        .is_none());
    assert!(index
        .resolve_exact("stage-a", "parent-a", "loom-a", "ambiguous")
        .is_none());
}

#[test]
fn exact_lookup_rejects_a_row_from_the_wrong_stage_directory() {
    let work_dir = tempfile::tempdir().unwrap();
    let wrong_stage = work_dir.path().join("subagents").join("stage-b");
    fs::create_dir_all(&wrong_stage).unwrap();
    fs::write(
        wrong_stage.join("starts.jsonl"),
        concat!(
            "{\"agent_id\":\"agent-x\",\"agent_type\":\"review\",",
            "\"parent_session_id\":\"parent-a\",\"stage_id\":\"stage-a\",",
            "\"loom_session_id\":\"loom-a\",\"ts\":\"2026-09-13T10:00:00Z\"}\n"
        ),
    )
    .unwrap();

    let index = StartedAgentTypeIndex::load(Some(work_dir.path()));

    assert!(index
        .resolve_exact("stage-a", "parent-a", "loom-a", "agent-x")
        .is_none());
}
