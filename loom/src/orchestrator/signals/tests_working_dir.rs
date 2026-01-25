//! working_dir and execution path tests

use super::super::cache::generate_stable_prefix;
use super::super::format::format_signal_content;
use super::{create_test_session, create_test_stage, create_test_worktree};
use super::super::types::EmbeddedContext;

#[test]
fn test_signal_contains_working_dir() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let content = format_signal_content(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Check working_dir is displayed in Target section
    assert!(content.contains("working_dir"));
    assert!(content.contains("`loom`"));
}

#[test]
fn test_signal_contains_execution_path() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let content = format_signal_content(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Check Execution Path is displayed
    assert!(content.contains("Execution Path"));
    // Should contain the computed path: worktree.path + working_dir
    assert!(content.contains("/repo/.worktrees/stage-1/loom"));
}

#[test]
fn test_signal_execution_path_default_working_dir() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = None; // Default to "."
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let content = format_signal_content(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Check working_dir defaults to "."
    assert!(content.contains("working_dir"));
    assert!(content.contains("`.`"));
    // Execution path should just be worktree path
    assert!(content.contains("/repo/.worktrees/stage-1"));
}

#[test]
fn test_signal_acceptance_criteria_working_dir_note() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let content = format_signal_content(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Check acceptance criteria section contains working_dir note
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("These commands will run from working_dir"));
    assert!(content.contains("`loom`"));
}

#[test]
fn test_signal_contains_where_commands_execute_box() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let content = format_signal_content(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Check the reminder box is present
    assert!(content.contains("WHERE COMMANDS EXECUTE"));
    assert!(content.contains("Acceptance criteria run from"));
    assert!(content.contains("WORKTREE + working_dir"));
}

#[test]
fn test_stable_prefix_contains_working_dir_reminder() {
    let prefix = generate_stable_prefix();

    // Check working_dir reminder is in Path Boundaries section
    assert!(prefix.contains("working_dir Reminder"));
    assert!(prefix.contains("WORKTREE + working_dir"));
    assert!(prefix.contains("execution path"));
}
