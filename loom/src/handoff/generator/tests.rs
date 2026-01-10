//! Tests for handoff generation functionality.

use std::fs;
use tempfile::TempDir;

use super::content::HandoffContent;
use super::formatter::format_handoff_markdown;
use super::generate_handoff;
use super::numbering::{find_latest_handoff, get_next_handoff_number};
use crate::models::session::Session;
use crate::models::stage::Stage;

#[test]
fn test_handoff_content_builder() {
    let content = HandoffContent::new("session-123".to_string(), "stage-456".to_string())
        .with_context_percent(75.5)
        .with_goals("Build feature X".to_string())
        .with_next_steps(vec!["Step 1".to_string(), "Step 2".to_string()]);

    assert_eq!(content.session_id, "session-123");
    assert_eq!(content.stage_id, "stage-456");
    assert_eq!(content.context_percent, 75.5);
    assert_eq!(content.goals, "Build feature X");
    assert_eq!(content.next_steps.len(), 2);
    assert!(content.git_history.is_none());
}

#[test]
fn test_format_handoff_markdown() {
    let content = HandoffContent::new("session-abc".to_string(), "stage-xyz".to_string())
        .with_context_percent(80.0)
        .with_goals("Implement authentication".to_string())
        .with_completed_work(vec!["Created login form".to_string()])
        .with_decisions(vec![(
            "Use JWT tokens".to_string(),
            "Industry standard".to_string(),
        )])
        .with_next_steps(vec!["Add token refresh".to_string()]);

    let markdown = format_handoff_markdown(&content).unwrap();

    assert!(markdown.contains("# Handoff: stage-xyz"));
    assert!(markdown.contains("**From**: session-abc"));
    assert!(markdown.contains("**Context**: 80.0%"));
    assert!(markdown.contains("Implement authentication"));
    assert!(markdown.contains("Created login form"));
    assert!(markdown.contains("Use JWT tokens"));
    assert!(markdown.contains("Add token refresh"));
}

#[test]
fn test_format_handoff_markdown_escapes_pipes() {
    let content = HandoffContent::new("session-1".to_string(), "stage-1".to_string())
        .with_decisions(vec![(
            "Choice with | pipe".to_string(),
            "Reason with | pipe".to_string(),
        )]);

    let markdown = format_handoff_markdown(&content).unwrap();

    // Should escape pipes in table cells
    assert!(markdown.contains(r"Choice with \| pipe"));
    assert!(markdown.contains(r"Reason with \| pipe"));
}

#[test]
fn test_format_handoff_markdown_with_git_history() {
    use crate::handoff::git_handoff::{CommitInfo, GitHistory};

    let git_history = GitHistory {
        branch: "loom/test-stage".to_string(),
        base_branch: "main".to_string(),
        commits: vec![
            CommitInfo {
                hash: "abc1234".to_string(),
                message: "Add new feature".to_string(),
            },
            CommitInfo {
                hash: "def5678".to_string(),
                message: "Fix bug".to_string(),
            },
        ],
        uncommitted_changes: vec!["M src/file.rs".to_string()],
    };

    let content = HandoffContent::new("session-abc".to_string(), "stage-xyz".to_string())
        .with_git_history(Some(git_history));

    let markdown = format_handoff_markdown(&content).unwrap();

    assert!(markdown.contains("## Git History"));
    assert!(markdown.contains("**Branch**: loom/test-stage (from main)"));
    assert!(markdown.contains("abc1234"));
    assert!(markdown.contains("Add new feature"));
    assert!(markdown.contains("M src/file.rs"));
}

#[test]
fn test_get_next_handoff_number_empty_dir() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();

    let number = get_next_handoff_number("stage-1", work_dir).unwrap();
    assert_eq!(number, 1);
}

#[test]
fn test_get_next_handoff_number_with_existing() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let handoffs_dir = work_dir.join("handoffs");
    fs::create_dir_all(&handoffs_dir).unwrap();

    // Create some existing handoff files
    fs::write(handoffs_dir.join("stage-1-handoff-001.md"), "test").unwrap();
    fs::write(handoffs_dir.join("stage-1-handoff-002.md"), "test").unwrap();
    fs::write(handoffs_dir.join("stage-2-handoff-001.md"), "test").unwrap();

    let number = get_next_handoff_number("stage-1", work_dir).unwrap();
    assert_eq!(number, 3);

    let number2 = get_next_handoff_number("stage-2", work_dir).unwrap();
    assert_eq!(number2, 2);
}

#[test]
fn test_find_latest_handoff() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let handoffs_dir = work_dir.join("handoffs");
    fs::create_dir_all(&handoffs_dir).unwrap();

    // Create some handoff files
    fs::write(handoffs_dir.join("stage-1-handoff-001.md"), "first").unwrap();
    fs::write(handoffs_dir.join("stage-1-handoff-002.md"), "second").unwrap();
    fs::write(handoffs_dir.join("stage-1-handoff-003.md"), "third").unwrap();

    let latest = find_latest_handoff("stage-1", work_dir).unwrap();
    assert!(latest.is_some());

    let latest_path = latest.unwrap();
    assert!(latest_path.ends_with("stage-1-handoff-003.md"));

    let content = fs::read_to_string(latest_path).unwrap();
    assert_eq!(content, "third");
}

#[test]
fn test_find_latest_handoff_none_exist() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();

    let latest = find_latest_handoff("stage-1", work_dir).unwrap();
    assert!(latest.is_none());
}

#[test]
fn test_generate_handoff() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();

    let session = Session::new();
    let stage = Stage::new("test-stage".to_string(), Some("Test stage".to_string()));

    let content = HandoffContent::new(session.id.clone(), stage.id.clone())
        .with_context_percent(75.0)
        .with_goals("Complete test".to_string());

    let handoff_path = generate_handoff(&session, &stage, content, work_dir).unwrap();

    assert!(handoff_path.exists());
    assert!(handoff_path.to_string_lossy().contains(&stage.id));
    assert!(handoff_path.to_string_lossy().contains("handoff-001.md"));

    let content = fs::read_to_string(&handoff_path).unwrap();
    assert!(content.contains("# Handoff:"));
    assert!(content.contains(&session.id));
    assert!(content.contains("Complete test"));
}

#[test]
fn test_generate_multiple_handoffs() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();

    let session = Session::new();
    let stage = Stage::new("test-stage".to_string(), None);

    let content1 = HandoffContent::new(session.id.clone(), stage.id.clone());
    let content2 = HandoffContent::new(session.id.clone(), stage.id.clone());

    let path1 = generate_handoff(&session, &stage, content1, work_dir).unwrap();
    let path2 = generate_handoff(&session, &stage, content2, work_dir).unwrap();

    assert!(path1.to_string_lossy().contains("handoff-001.md"));
    assert!(path2.to_string_lossy().contains("handoff-002.md"));
}
