use super::execution_models_for_stage;
use crate::fs::work_dir::WorkDir;

fn test_work_dir() -> (tempfile::TempDir, WorkDir) {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = WorkDir::new(temp.path().join(".loom/work")).unwrap();
    (temp, work_dir)
}

#[test]
fn forwarder_rows_are_skipped_and_codex_rows_counted() {
    let (_temp, work_dir) = test_work_dir();
    let stage_dir = work_dir.root().join("subagents").join("s1");
    std::fs::create_dir_all(&stage_dir).unwrap();
    std::fs::write(
        stage_dir.join("spawns.jsonl"),
        concat!(
            r#"{"agent_type":"loom-software-engineer","model":"sonnet"}"#,
            "\n",
            r#"{"agent_type":"loom-codex-forwarder","model":"sonnet"}"#,
            "\n",
            r#"{"agent_type":"loom-senior-software-engineer","model":"opus"}"#,
            "\n",
        ),
    )
    .unwrap();
    std::fs::write(
        stage_dir.join("codex.jsonl"),
        concat!(
            r#"{"model":"gpt-6.1-terra"}"#,
            "\n",
            r#"{"model":"gpt-6.1-terra"}"#,
            "\n",
            r#"{"model":"gpt-6-luna"}"#,
            "\n",
        ),
    )
    .unwrap();

    assert_eq!(
        execution_models_for_stage(&work_dir, "s1"),
        [
            "sonnet",
            "opus",
            "gpt-6.1-terra (requested)",
            "gpt-6-luna (requested)"
        ]
    );
}

#[test]
fn codex_model_family_and_future_names_are_preserved() {
    let (_temp, work_dir) = test_work_dir();
    let stage_dir = work_dir.root().join("subagents").join("s1");
    std::fs::create_dir_all(&stage_dir).unwrap();
    std::fs::write(
        stage_dir.join("codex.jsonl"),
        concat!(
            r#"{"model":"gpt-6-sol"}"#,
            "\n",
            r#"{"model":"gpt-6-astra"}"#,
            "\n",
            r#"{"model":"gpt-7-orbit"}"#,
            "\n",
            r#"{"v":2,"model":"gpt-unmatched"}"#,
            "\n",
        ),
    )
    .unwrap();

    assert_eq!(
        execution_models_for_stage(&work_dir, "s1"),
        [
            "gpt-6-sol (requested)",
            "gpt-6-astra (requested)",
            "gpt-7-orbit (requested)"
        ]
    );
}

#[test]
fn missing_stage_directory_contributes_nothing() {
    let (_temp, work_dir) = test_work_dir();

    assert!(execution_models_for_stage(&work_dir, "s1").is_empty());
}

#[test]
fn blank_and_malformed_rows_are_skipped() {
    let (_temp, work_dir) = test_work_dir();
    let stage_dir = work_dir.root().join("subagents").join("s1");
    std::fs::create_dir_all(&stage_dir).unwrap();
    std::fs::write(
        stage_dir.join("spawns.jsonl"),
        "\nnot json\n{\"model\":\"claude-haiku-4-5-20251001\"}\n\n{\"model\":\"sonnet\"}\n",
    )
    .unwrap();
    std::fs::write(
        stage_dir.join("codex.jsonl"),
        "not json\n{\"model\":\"gpt-6.1-terra\"}\n",
    )
    .unwrap();

    assert_eq!(
        execution_models_for_stage(&work_dir, "s1"),
        ["haiku-4-5", "sonnet", "gpt-6.1-terra (requested)"]
    );
}

#[test]
fn unsafe_stage_ids_contribute_nothing() {
    let (_temp, work_dir) = test_work_dir();

    assert!(execution_models_for_stage(&work_dir, "bad/stage").is_empty());
    assert!(execution_models_for_stage(&work_dir, ".").is_empty());
    assert!(execution_models_for_stage(&work_dir, "..").is_empty());
}

#[test]
fn names_differing_only_in_invisible_characters_are_one_model() {
    let (_temp, work_dir) = test_work_dir();
    let stage_dir = work_dir.root().join("subagents").join("s1");
    std::fs::create_dir_all(&stage_dir).unwrap();
    std::fs::write(
        stage_dir.join("spawns.jsonl"),
        concat!(
            r#"{"model":"sonnet\u200b"}"#,
            "\n",
            r#"{"model":"sonnet "}"#,
            "\n",
            r#"{"model":"   "}"#,
            "\n",
            r#"{"model":"claude-haiku-4-5-20251001\u200b"}"#,
            "\n",
        ),
    )
    .unwrap();

    assert_eq!(
        execution_models_for_stage(&work_dir, "s1"),
        ["sonnet", "haiku-4-5"]
    );
}

#[test]
fn a_multibyte_model_name_does_not_panic() {
    let (_temp, work_dir) = test_work_dir();
    let stage_dir = work_dir.root().join("subagents").join("s1");
    std::fs::create_dir_all(&stage_dir).unwrap();
    std::fs::write(
        stage_dir.join("spawns.jsonl"),
        concat!(
            r#"{"model":"aãaaaaaaaa"}"#,
            "\n",
            r#"{"model":"claude-haiku-4-5-20251001"}"#,
            "\n",
        ),
    )
    .unwrap();

    assert_eq!(
        execution_models_for_stage(&work_dir, "s1"),
        ["aãaaaaaaaa", "haiku-4-5"]
    );
}

#[test]
fn fifo_ledgers_return_without_a_writer() {
    let (_temp, work_dir) = test_work_dir();
    let stage_dir = work_dir.root().join("subagents/s1");
    std::fs::create_dir_all(&stage_dir).unwrap();
    for name in ["spawns.jsonl", "codex.jsonl"] {
        nix::unistd::mkfifo(&stage_dir.join(name), nix::sys::stat::Mode::S_IRUSR).unwrap();
    }
    assert!(execution_models_for_stage(&work_dir, "s1").is_empty());
}

#[test]
fn worktree_state_root_symlink_keeps_models_readable() {
    let (_temp, work_dir) = test_work_dir();
    let alias = tempfile::tempdir().unwrap();
    let loom_dir = alias.path().join(".loom");
    std::fs::create_dir(&loom_dir).unwrap();
    std::os::unix::fs::symlink(work_dir.root(), loom_dir.join("work")).unwrap();
    let linked_work = WorkDir::new(loom_dir.join("work")).unwrap();
    let stage_dir = work_dir.root().join("subagents/s1");
    std::fs::create_dir_all(&stage_dir).unwrap();
    std::fs::write(stage_dir.join("spawns.jsonl"), "{\"model\":\"sonnet\"}\n").unwrap();

    assert_eq!(execution_models_for_stage(&linked_work, "s1"), ["sonnet"]);
}

#[test]
fn symlinked_stage_is_refused_and_regular_ledgers_keep_the_byte_cap() {
    let (_temp, work_dir) = test_work_dir();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("spawns.jsonl"),
        "{\"model\":\"unsafe\"}\n",
    )
    .unwrap();
    std::fs::create_dir_all(work_dir.root().join("subagents")).unwrap();
    std::os::unix::fs::symlink(outside.path(), work_dir.root().join("subagents/s1")).unwrap();
    assert!(execution_models_for_stage(&work_dir, "s1").is_empty());

    std::fs::remove_file(work_dir.root().join("subagents/s1")).unwrap();
    let stage_dir = work_dir.root().join("subagents/s1");
    std::fs::create_dir(&stage_dir).unwrap();
    let ledger = format!(
        "{{\"model\":\"sonnet\"}}\n{}",
        " ".repeat(super::MAX_LEDGER_BYTES as usize)
    );
    std::fs::write(stage_dir.join("spawns.jsonl"), ledger).unwrap();
    assert_eq!(execution_models_for_stage(&work_dir, "s1"), ["sonnet"]);
}
