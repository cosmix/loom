use std::fs;

use super::{RequestedModels, AMBIGUOUS_MODEL};

#[test]
fn requested_model_ledger_refuses_oversize_fifo_and_parent_symlink() {
    use std::os::unix::ffi::OsStrExt;
    let root = tempfile::tempdir().unwrap();
    let stage = root.path().join("subagents/stage");
    std::fs::create_dir_all(&stage).unwrap();
    let path = stage.join("spawns.jsonl");
    std::fs::write(&path, vec![b' '; super::MAX_LEDGER_BYTES + 1]).unwrap();
    assert!(super::json_lines(&path).is_empty());
    std::fs::remove_file(&path).unwrap();
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated pathname.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    assert!(super::json_lines(&path).is_empty());
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("spawns.jsonl"), "{}\n").unwrap();
    let link = root.path().join("subagents/link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    assert!(super::json_lines(&link.join("spawns.jsonl")).is_empty());
}

fn write_ledger(work_dir: &tempfile::TempDir, name: &str, rows: &str) {
    let stage_dir = work_dir.path().join("subagents/stage-a");
    fs::create_dir_all(&stage_dir).unwrap();
    fs::write(stage_dir.join(name), rows).unwrap();
}

#[test]
fn requested_models_joins_one_spawn_to_one_start() {
    let work_dir = tempfile::tempdir().unwrap();
    write_ledger(
        &work_dir,
        "spawns.jsonl",
        r#"{"ts":"2026-10-01T00:00:00Z","session_id":"loom-a","agent_type":"loom-software-engineer","model":"sonnet"}
"#,
    );
    write_ledger(
        &work_dir,
        "starts.jsonl",
        r#"{"ts":"2026-10-01T00:00:01Z","agent_id":"agent-a","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}
"#,
    );

    let requested = RequestedModels::load(Some(work_dir.path()));

    assert_eq!(
        requested.requested_model("agent-a", "parent-a"),
        Some("sonnet")
    );
}

#[test]
fn requested_models_marks_parallel_same_timestamp_spawns_ambiguous() {
    let work_dir = tempfile::tempdir().unwrap();
    write_ledger(
        &work_dir,
        "spawns.jsonl",
        concat!(
            r#"{"ts":"2026-10-01T00:00:00Z","session_id":"loom-a","agent_type":"loom-software-engineer","model":"sonnet"}"#,
            "\n",
            r#"{"ts":"2026-10-01T00:00:00Z","session_id":"loom-a","agent_type":"loom-software-engineer","model":"opus"}"#,
            "\n",
        ),
    );
    write_ledger(
        &work_dir,
        "starts.jsonl",
        concat!(
            r#"{"ts":"2026-10-01T00:00:01Z","agent_id":"agent-a","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}"#,
            "\n",
            r#"{"ts":"2026-10-01T00:00:02Z","agent_id":"agent-b","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}"#,
            "\n",
        ),
    );

    let requested = RequestedModels::load(Some(work_dir.path()));

    assert_eq!(
        requested.requested_model("agent-a", "parent-a"),
        Some(AMBIGUOUS_MODEL)
    );
    assert_eq!(
        requested.requested_model("agent-b", "parent-a"),
        Some(AMBIGUOUS_MODEL)
    );
}

#[test]
fn requested_models_pairs_distinct_timestamps_in_order() {
    let work_dir = tempfile::tempdir().unwrap();
    write_ledger(
        &work_dir,
        "spawns.jsonl",
        concat!(
            r#"{"ts":"2026-10-01T00:00:00Z","session_id":"loom-a","agent_type":"loom-software-engineer","model":"haiku"}"#,
            "\n",
            r#"{"ts":"2026-10-01T00:00:01Z","session_id":"loom-a","agent_type":"loom-software-engineer","model":"sonnet"}"#,
            "\n",
        ),
    );
    write_ledger(
        &work_dir,
        "starts.jsonl",
        concat!(
            r#"{"ts":"2026-10-01T00:00:02Z","agent_id":"agent-a","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}"#,
            "\n",
            r#"{"ts":"2026-10-01T00:00:03Z","agent_id":"agent-b","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}"#,
            "\n",
        ),
    );

    let requested = RequestedModels::load(Some(work_dir.path()));

    assert_eq!(
        requested.requested_model("agent-a", "parent-a"),
        Some("haiku")
    );
    assert_eq!(
        requested.requested_model("agent-b", "parent-a"),
        Some("sonnet")
    );
}

#[test]
fn requested_models_leaves_unmatched_and_missing_ledgers_unknown() {
    let work_dir = tempfile::tempdir().unwrap();
    write_ledger(
        &work_dir,
        "starts.jsonl",
        r#"{"ts":"2026-10-01T00:00:00Z","agent_id":"agent-a","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}
"#,
    );

    let unmatched = RequestedModels::load(Some(work_dir.path()));
    let missing = RequestedModels::load(None);

    assert_eq!(unmatched.requested_model("agent-a", "parent-a"), None);
    assert_eq!(missing.requested_model("agent-a", "parent-a"), None);
}

#[test]
fn requested_models_does_not_give_an_empty_model_spawn_the_named_model() {
    let work_dir = tempfile::tempdir().unwrap();
    write_ledger(
        &work_dir,
        "spawns.jsonl",
        concat!(
            r#"{"ts":"2026-10-01T00:00:00Z","session_id":"loom-a","agent_type":"loom-software-engineer","model":"sonnet"}"#,
            "\n",
            r#"{"ts":"2026-10-01T00:00:01Z","session_id":"loom-a","agent_type":"loom-software-engineer","model":""}"#,
            "\n",
        ),
    );
    write_ledger(
        &work_dir,
        "starts.jsonl",
        concat!(
            r#"{"ts":"2026-10-01T00:00:02Z","agent_id":"agent-a","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}"#,
            "\n",
            r#"{"ts":"2026-10-01T00:00:03Z","agent_id":"agent-b","agent_type":"loom-software-engineer","parent_session_id":"parent-a","loom_session_id":"loom-a"}"#,
            "\n",
        ),
    );

    let requested = RequestedModels::load(Some(work_dir.path()));

    assert_eq!(
        requested.requested_model("agent-a", "parent-a"),
        Some("sonnet")
    );
    assert_eq!(requested.requested_model("agent-b", "parent-a"), None);
}
