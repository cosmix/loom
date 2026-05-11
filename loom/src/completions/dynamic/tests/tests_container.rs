//! Tests for container subcommand completions

use super::super::commands::{complete_flags, complete_subcommands};
use super::super::*;
use super::setup_test_workspace;

#[test]
fn test_complete_subcommands_container_includes_list_and_logs() {
    let results = complete_subcommands("container", "").unwrap();
    assert!(
        results.contains(&"list".to_string()),
        "expected 'list' in container subcommands, got: {results:?}"
    );
    assert!(
        results.contains(&"logs".to_string()),
        "expected 'logs' in container subcommands, got: {results:?}"
    );
}

#[test]
fn test_complete_flags_container_list_contains_all_and_json() {
    let results = complete_flags(&["container", "list"], "").unwrap();
    assert!(
        results.contains(&"--all".to_string()),
        "expected '--all' in container list flags, got: {results:?}"
    );
    assert!(
        results.contains(&"--json".to_string()),
        "expected '--json' in container list flags, got: {results:?}"
    );
}

#[test]
fn test_complete_flags_container_logs_contains_follow_and_tail() {
    let results = complete_flags(&["container", "logs"], "").unwrap();
    assert!(
        results.contains(&"--follow".to_string()),
        "expected '--follow' in container logs flags, got: {results:?}"
    );
    assert!(
        results.contains(&"--tail".to_string()),
        "expected '--tail' in container logs flags, got: {results:?}"
    );
}

#[test]
fn test_container_shell_completes_stage_ids() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom container shell ".to_string(),
        current_word: "".to_string(),
        prev_word: "shell".to_string(),
    };

    let words = parse_cmdline_words(&ctx.cmdline);
    let cmd_path = extract_command_path(&words, &ctx.current_word);
    let results =
        complete_after_subcommand(root, &ctx.current_word, cmd_path[0], cmd_path[1]).unwrap();

    assert!(
        results.contains(&"core-architecture".to_string()),
        "expected stage IDs for container shell, got: {results:?}"
    );
    assert!(
        results.contains(&"math-core".to_string()),
        "expected stage IDs for container shell, got: {results:?}"
    );
}

#[test]
fn test_container_logs_completes_stage_ids() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom container logs ".to_string(),
        current_word: "".to_string(),
        prev_word: "logs".to_string(),
    };

    let words = parse_cmdline_words(&ctx.cmdline);
    let cmd_path = extract_command_path(&words, &ctx.current_word);
    let results =
        complete_after_subcommand(root, &ctx.current_word, cmd_path[0], cmd_path[1]).unwrap();

    assert!(
        results.contains(&"core-architecture".to_string()),
        "expected stage IDs for container logs, got: {results:?}"
    );
    assert!(
        results.contains(&"integration".to_string()),
        "expected stage IDs for container logs, got: {results:?}"
    );
}

#[test]
fn test_container_logs_tail_flag_suppresses_stage_ids() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    // Simulate: loom container logs --tail <tab>
    // prev_word is "--tail", so complete_flag_value should return empty vec
    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom container logs --tail ".to_string(),
        current_word: "".to_string(),
        prev_word: "--tail".to_string(),
    };

    let completions = route_completion(
        root,
        &ctx.current_word,
        &ctx.prev_word,
        &["container", "logs"],
        &ctx.cmdline,
    )
    .unwrap();

    assert!(
        completions.is_empty(),
        "expected no completions after --tail (should not suggest stage IDs), got: {completions:?}"
    );
}
