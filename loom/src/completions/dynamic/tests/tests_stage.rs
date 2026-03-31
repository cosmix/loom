//! Tests for stage subcommand completions

use super::super::stages::complete_stage_ids_filtered;
use super::super::*;
use super::setup_test_workspace;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_complete_dynamic_stage_hold() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage hold".to_string(),
        current_word: "core".to_string(),
        prev_word: "hold".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_release() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage release".to_string(),
        current_word: "".to_string(),
        prev_word: "release".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_skip() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage skip".to_string(),
        current_word: "ui".to_string(),
        prev_word: "skip".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_retry() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage retry".to_string(),
        current_word: "".to_string(),
        prev_word: "retry".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_verify() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage verify".to_string(),
        current_word: "".to_string(),
        prev_word: "verify".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_merge() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage merge".to_string(),
        current_word: "".to_string(),
        prev_word: "merge".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_resume() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage resume".to_string(),
        current_word: "".to_string(),
        prev_word: "resume".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_output_set() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output set".to_string(),
        current_word: "".to_string(),
        prev_word: "set".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_output_get() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output get".to_string(),
        current_word: "math".to_string(),
        prev_word: "get".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_output_list() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output list".to_string(),
        current_word: "".to_string(),
        prev_word: "list".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_output_remove() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output remove".to_string(),
        current_word: "ui".to_string(),
        prev_word: "remove".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

// Status-filtered stage completion tests

#[test]
fn test_complete_stage_ids_filtered_executing() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let stages_dir = root.join(".work/stages");
    fs::create_dir_all(&stages_dir).unwrap();

    fs::write(
        stages_dir.join("01-build.md"),
        "---\nstatus: executing\n---\n",
    )
    .unwrap();
    fs::write(
        stages_dir.join("02-test.md"),
        "---\nstatus: completed\n---\n",
    )
    .unwrap();
    fs::write(
        stages_dir.join("03-deploy.md"),
        "---\nstatus: blocked\n---\n",
    )
    .unwrap();

    let results = complete_stage_ids_filtered(root, "", &["executing"]).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"build".to_string()));
}

#[test]
fn test_complete_stage_ids_filtered_multiple_statuses() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let stages_dir = root.join(".work/stages");
    fs::create_dir_all(&stages_dir).unwrap();

    fs::write(
        stages_dir.join("01-build.md"),
        "---\nstatus: executing\n---\n",
    )
    .unwrap();
    fs::write(
        stages_dir.join("02-test.md"),
        "---\nstatus: completed\n---\n",
    )
    .unwrap();
    fs::write(
        stages_dir.join("03-deploy.md"),
        "---\nstatus: blocked\n---\n",
    )
    .unwrap();

    let results = complete_stage_ids_filtered(root, "", &["executing", "blocked"]).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.contains(&"build".to_string()));
    assert!(results.contains(&"deploy".to_string()));
}

#[test]
fn test_complete_stage_ids_filtered_empty_returns_all() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let stages_dir = root.join(".work/stages");
    fs::create_dir_all(&stages_dir).unwrap();

    fs::write(
        stages_dir.join("01-build.md"),
        "---\nstatus: executing\n---\n",
    )
    .unwrap();
    fs::write(
        stages_dir.join("02-test.md"),
        "---\nstatus: completed\n---\n",
    )
    .unwrap();
    fs::write(
        stages_dir.join("03-deploy.md"),
        "---\nstatus: blocked\n---\n",
    )
    .unwrap();

    let results = complete_stage_ids_filtered(root, "", &[]).unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.contains(&"build".to_string()));
    assert!(results.contains(&"test".to_string()));
    assert!(results.contains(&"deploy".to_string()));
}
