//! Merge signal tests

use std::fs;
use tempfile::TempDir;

use super::{create_test_session, create_test_stage, create_test_worktree};
use super::super::generate::generate_signal;
use super::super::merge::{
    format_merge_signal_content, generate_merge_signal, parse_merge_signal_content,
    read_merge_signal,
};

#[test]
fn test_generate_merge_signal_basic() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let conflicting_files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];

    let result = generate_merge_signal(
        &session,
        &stage,
        "loom/stage-1",
        "main",
        &conflicting_files,
        &work_dir,
    );

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    assert!(signal_path.exists());

    let content = fs::read_to_string(&signal_path).unwrap();
    assert!(content.contains("# Merge Signal: session-test-123"));
    assert!(content.contains("- **Session**: session-test-123"));
    assert!(content.contains("- **Stage**: stage-1"));
    assert!(content.contains("- **Source Branch**: loom/stage-1"));
    assert!(content.contains("- **Target Branch**: main"));
    assert!(content.contains("## Conflicting Files"));
    assert!(content.contains("- `src/main.rs`"));
    assert!(content.contains("- `src/lib.rs`"));
}

#[test]
fn test_generate_merge_signal_empty_conflicts() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();

    let result = generate_merge_signal(&session, &stage, "loom/stage-1", "main", &[], &work_dir);

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Conflicting Files"));
    assert!(content.contains("_No specific files listed"));
}

#[test]
fn test_format_merge_signal_content_sections() {
    let session = create_test_session();
    let stage = create_test_stage();
    let conflicting_files = vec!["src/test.rs".to_string()];

    let content =
        format_merge_signal_content(&session, &stage, "loom/stage-1", "main", &conflicting_files);

    // Check all required sections are present
    assert!(content.contains("# Merge Signal:"));
    assert!(content.contains("## Merge Context"));
    assert!(content.contains("## Execution Rules"));
    assert!(content.contains("## Target"));
    assert!(content.contains("## Stage Context"));
    assert!(content.contains("## Conflicting Files"));
    assert!(content.contains("## Your Task"));
    assert!(content.contains("## Important"));

    // Check key instructions
    assert!(content.contains("git merge loom/stage-1"));
    assert!(content.contains("Resolve conflicts"));
    assert!(content.contains("git add"));
    assert!(content.contains("git commit"));
    // Should use worktree remove for cleanup, not loom merge
    assert!(content.contains("loom worktree remove stage-1"));
}

#[test]
fn test_read_merge_signal() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let conflicting_files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];

    generate_merge_signal(
        &session,
        &stage,
        "loom/stage-1",
        "main",
        &conflicting_files,
        &work_dir,
    )
    .unwrap();

    let result = read_merge_signal("session-test-123", &work_dir);
    assert!(result.is_ok());

    let signal_content = result.unwrap();
    assert!(signal_content.is_some());

    let content = signal_content.unwrap();
    assert_eq!(content.session_id, "session-test-123");
    assert_eq!(content.stage_id, "stage-1");
    assert_eq!(content.source_branch, "loom/stage-1");
    assert_eq!(content.target_branch, "main");
    assert_eq!(content.conflicting_files.len(), 2);
    assert!(content
        .conflicting_files
        .contains(&"src/main.rs".to_string()));
    assert!(content
        .conflicting_files
        .contains(&"src/lib.rs".to_string()));
}

#[test]
fn test_read_merge_signal_returns_none_for_regular_signal() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    // Generate a regular signal (not a merge signal)
    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    // read_merge_signal should return None for regular signals
    let result = read_merge_signal("session-test-123", &work_dir);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_read_merge_signal_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let result = read_merge_signal("nonexistent-session", &work_dir);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_parse_merge_signal_content() {
    let content = r#"# Merge Signal: session-merge-123

## Merge Context

You are resolving a **merge conflict** in the main repository.

## Target

- **Session**: session-merge-123
- **Stage**: feature-stage
- **Source Branch**: loom/feature-stage
- **Target Branch**: develop

## Conflicting Files

- `src/app.rs`
- `src/config.rs`

## Your Task

1. Run: `git merge loom/feature-stage`
"#;

    let result = parse_merge_signal_content("session-merge-123", content);
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert_eq!(parsed.session_id, "session-merge-123");
    assert_eq!(parsed.stage_id, "feature-stage");
    assert_eq!(parsed.source_branch, "loom/feature-stage");
    assert_eq!(parsed.target_branch, "develop");
    assert_eq!(parsed.conflicting_files.len(), 2);
    assert_eq!(parsed.conflicting_files[0], "src/app.rs");
    assert_eq!(parsed.conflicting_files[1], "src/config.rs");
}
