//! working_dir and execution path tests

use super::super::cache::generate_stable_prefix;
use super::super::format::format_signal_content;
use super::super::types::EmbeddedContext;
use super::{create_test_session, create_test_stage, create_test_worktree};

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
    // Points at the Target section for the exact path rather than restating it
    assert!(prefix.contains("for the exact path"));
}

#[test]
fn test_signal_contains_worktree_isolation_section() {
    let session = create_test_session();
    let stage = create_test_stage();
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

    // Check Worktree Isolation section header
    assert!(content.contains("## Worktree Isolation"));

    // Check it shows relative worktree path
    assert!(content.contains(".worktrees/stage-1/"));

    // Check ALLOWED list
    assert!(content.contains("**ALLOWED:**"));
    assert!(content.contains("Files within this worktree"));
    assert!(content.contains("`.loom/work/` directory (via symlink)"));
    assert!(content.contains("Reading `CLAUDE.md` (symlinked)"));
    assert!(content.contains("Using loom CLI commands"));

    // Check FORBIDDEN list
    assert!(content.contains("**FORBIDDEN:**"));
    assert!(content.contains("Path traversal"));
    assert!(content.contains("Git operations targeting main repo"));
    assert!(
        content.contains("Direct modification of `.loom/work/stages/` or `.loom/work/sessions/`")
    );
    assert!(content.contains("Attempting to merge your own branch"));

    // Check guidance about stopping
    assert!(content.contains("STOP"));
    assert!(content.contains("orchestrator will handle cross-worktree operations"));
}
