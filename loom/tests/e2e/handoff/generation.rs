//! Handoff generation tests

use loom::handoff::generator::{generate_handoff, HandoffContent};
use loom::models::constants::DEFAULT_CONTEXT_LIMIT;
use loom::models::session::Session;
use loom::models::stage::Stage;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_handoff_generation() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    // Create session approaching context limit
    let mut session = Session::new();
    session.context_limit = DEFAULT_CONTEXT_LIMIT;
    session.context_tokens = 160_000; // 80% usage

    // Create associated stage
    let stage = Stage::new(
        "test-feature".to_string(),
        Some("Test feature implementation".to_string()),
    );

    // Generate handoff content
    let content = HandoffContent::new(session.id.clone(), stage.id.clone())
        .with_context_percent(session.context_health())
        .with_goals("Implement test feature with proper error handling".to_string())
        .with_completed_work(vec![
            "Created initial module structure in src/test_feature.rs:1-50".to_string(),
            "Implemented core logic in src/test_feature.rs:51-120".to_string(),
        ])
        .with_decisions(vec![(
            "Use Result<T, E> for error handling".to_string(),
            "Follows Rust best practices".to_string(),
        )])
        .with_current_branch(Some("feat-test-feature".to_string()))
        .with_test_status(Some("2 passing, 1 pending".to_string()))
        .with_files_modified(vec![
            "src/test_feature.rs".to_string(),
            "tests/test_feature_tests.rs".to_string(),
        ])
        .with_next_steps(vec![
            "Complete error handling tests in tests/test_feature_tests.rs:45+".to_string(),
            "Add documentation to src/test_feature.rs:1-10".to_string(),
        ])
        .with_learnings(vec![
            "Module uses async/await pattern throughout".to_string()
        ]);

    // Generate handoff
    let handoff_path =
        generate_handoff(&session, &stage, content, work_dir).expect("Should generate handoff");

    // Verify handoff file was created
    assert!(handoff_path.exists());
    assert!(handoff_path.to_string_lossy().contains(&stage.id));
    assert!(handoff_path.to_string_lossy().contains("handoff-001.md"));

    // Verify handoff file location
    assert!(handoff_path.starts_with(work_dir.join("handoffs")));

    // Read and verify handoff contents
    let content = fs::read_to_string(&handoff_path).expect("Should read handoff file");

    // Verify required fields are present
    assert!(content.contains(&format!("# Handoff: {}", stage.id)));
    assert!(content.contains(&session.id));
    assert!(content.contains("80.0%") || content.contains("80%"));
    assert!(content.contains("Implement test feature"));
    assert!(content.contains("Created initial module structure"));
    assert!(content.contains("src/test_feature.rs"));
    assert!(content.contains("feat-test-feature"));
}

#[test]
fn test_handoff_file_naming() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let session = Session::new();
    let stage = Stage::new("naming-test".to_string(), None);

    // Generate first handoff
    let content1 =
        HandoffContent::new(session.id.clone(), stage.id.clone()).with_context_percent(75.0);

    let handoff_path1 = generate_handoff(&session, &stage, content1, work_dir)
        .expect("Should generate first handoff");

    assert!(handoff_path1.to_string_lossy().contains("handoff-001.md"));

    // Generate second handoff
    let content2 =
        HandoffContent::new(session.id.clone(), stage.id.clone()).with_context_percent(80.0);

    let handoff_path2 = generate_handoff(&session, &stage, content2, work_dir)
        .expect("Should generate second handoff");

    assert!(handoff_path2.to_string_lossy().contains("handoff-002.md"));

    // Generate third handoff
    let content3 =
        HandoffContent::new(session.id.clone(), stage.id.clone()).with_context_percent(85.0);

    let handoff_path3 = generate_handoff(&session, &stage, content3, work_dir)
        .expect("Should generate third handoff");

    assert!(handoff_path3.to_string_lossy().contains("handoff-003.md"));

    // Verify all files exist
    assert!(handoff_path1.exists());
    assert!(handoff_path2.exists());
    assert!(handoff_path3.exists());
}

#[test]
fn test_handoff_includes_required_fields() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let session = Session::new();
    let mut stage = Stage::new(
        "required-fields-test".to_string(),
        Some("Testing required fields".to_string()),
    );
    stage.id = "stage-required-001".to_string();

    let content = HandoffContent::new(session.id.clone(), stage.id.clone())
        .with_context_percent(78.5)
        .with_goals("Test all required fields".to_string())
        .with_plan_id(Some("plan-abc-123".to_string()));

    let handoff_path =
        generate_handoff(&session, &stage, content, work_dir).expect("Should generate handoff");

    let handoff_content = fs::read_to_string(&handoff_path).expect("Should read handoff");

    // Verify metadata section
    assert!(handoff_content.contains("## Metadata"));
    assert!(handoff_content.contains("- **Date**: "));
    assert!(handoff_content.contains(&format!("- **From**: {}", session.id)));
    assert!(handoff_content.contains("- **To**: (next session)"));
    assert!(handoff_content.contains(&format!("- **Stage**: {}", stage.id)));
    assert!(handoff_content.contains("- **Plan**: plan-abc-123"));
    assert!(handoff_content.contains("- **Context**: 78.5%"));

    // Verify section headers
    assert!(handoff_content.contains("## Goals (What We're Building)"));
    assert!(handoff_content.contains("## Completed Work"));
    assert!(handoff_content.contains("## Key Decisions Made"));
    assert!(handoff_content.contains("## Current State"));
    assert!(handoff_content.contains("## Next Steps (Prioritized)"));
    assert!(handoff_content.contains("## Learnings / Patterns Identified"));

    // Verify content
    assert!(handoff_content.contains("Test all required fields"));
}

#[test]
fn test_handoff_content_builder_chain() {
    let session_id = "session-test-123".to_string();
    let stage_id = "stage-test-456".to_string();

    let content = HandoffContent::new(session_id.clone(), stage_id.clone())
        .with_context_percent(82.3)
        .with_goals("Build comprehensive test".to_string())
        .with_completed_work(vec!["Task 1".to_string(), "Task 2".to_string()])
        .with_decisions(vec![
            ("Decision A".to_string(), "Reason A".to_string()),
            ("Decision B".to_string(), "Reason B".to_string()),
        ])
        .with_current_branch(Some("test-branch".to_string()))
        .with_test_status(Some("all passing".to_string()))
        .with_files_modified(vec!["file1.rs".to_string(), "file2.rs".to_string()])
        .with_next_steps(vec![
            "Step 1".to_string(),
            "Step 2".to_string(),
            "Step 3".to_string(),
        ])
        .with_learnings(vec!["Learning 1".to_string()])
        .with_plan_id(Some("plan-xyz".to_string()));

    assert_eq!(content.session_id, session_id);
    assert_eq!(content.stage_id, stage_id);
    assert_eq!(content.context_percent, 82.3);
    assert_eq!(content.goals, "Build comprehensive test");
    assert_eq!(content.completed_work.len(), 2);
    assert_eq!(content.decisions.len(), 2);
    assert_eq!(content.current_branch, Some("test-branch".to_string()));
    assert_eq!(content.test_status, Some("all passing".to_string()));
    assert_eq!(content.files_modified.len(), 2);
    assert_eq!(content.next_steps.len(), 3);
    assert_eq!(content.learnings.len(), 1);
    assert_eq!(content.plan_id, Some("plan-xyz".to_string()));
}

#[test]
fn test_multiple_stages_different_handoffs() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let session = Session::new();

    // Create two different stages
    let stage1 = Stage::new("feature-a".to_string(), None);
    let stage2 = Stage::new("feature-b".to_string(), None);

    // Generate handoffs for each
    let content1 =
        HandoffContent::new(session.id.clone(), stage1.id.clone()).with_context_percent(75.0);

    let content2 =
        HandoffContent::new(session.id.clone(), stage2.id.clone()).with_context_percent(80.0);

    let handoff1 = generate_handoff(&session, &stage1, content1, work_dir)
        .expect("Should generate handoff for stage 1");

    let handoff2 = generate_handoff(&session, &stage2, content2, work_dir)
        .expect("Should generate handoff for stage 2");

    // Verify both files exist and have different names
    assert!(handoff1.exists());
    assert!(handoff2.exists());
    assert_ne!(handoff1, handoff2);

    // Verify each handoff is for the correct stage
    assert!(handoff1.to_string_lossy().contains(&stage1.id));
    assert!(handoff2.to_string_lossy().contains(&stage2.id));
}

#[test]
fn test_handoff_empty_fields() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();

    let session = Session::new();
    let stage = Stage::new("minimal-test".to_string(), None);

    // Create minimal handoff with only required fields
    let content = HandoffContent::new(session.id.clone(), stage.id.clone());

    let handoff_path = generate_handoff(&session, &stage, content, work_dir)
        .expect("Should generate minimal handoff");

    let handoff_content = fs::read_to_string(&handoff_path).expect("Should read handoff");

    // Verify it handles empty fields gracefully
    assert!(handoff_content.contains("No goals specified"));
    assert!(handoff_content.contains("No work completed yet"));
    assert!(handoff_content.contains("No decisions documented"));
    assert!(handoff_content.contains("Review current state and determine next actions"));
    assert!(handoff_content.contains("No learnings documented yet"));
}
