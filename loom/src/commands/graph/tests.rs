//! Tests for graph display functionality

use crate::models::stage::{Stage, StageStatus};

use super::display::build_graph_display;
use super::indicators::status_indicator;
use super::levels::compute_stage_levels;
use super::tree::build_tree_display;

fn create_test_stage(id: &str, name: &str, status: StageStatus, deps: Vec<&str>) -> Stage {
    let mut stage = Stage::new(name.to_string(), Some(format!("Test stage: {name}")));
    stage.id = id.to_string();
    stage.status = status;
    stage.dependencies = deps.into_iter().map(String::from).collect();
    stage
}

#[test]
fn test_status_indicator() {
    // Test that indicators contain the expected Unicode symbols
    // (colored strings include ANSI codes, so we check the base character)
    assert!(status_indicator(&StageStatus::Completed)
        .to_string()
        .contains('✓'));
    assert!(status_indicator(&StageStatus::Executing)
        .to_string()
        .contains('●'));
    assert!(status_indicator(&StageStatus::Queued)
        .to_string()
        .contains('▶'));
    assert!(status_indicator(&StageStatus::WaitingForDeps)
        .to_string()
        .contains('○'));
    assert!(status_indicator(&StageStatus::WaitingForInput)
        .to_string()
        .contains('?'));
    assert!(status_indicator(&StageStatus::Blocked)
        .to_string()
        .contains('✗'));
    assert!(status_indicator(&StageStatus::NeedsHandoff)
        .to_string()
        .contains('⟳'));
}

#[test]
fn test_build_graph_display_empty() {
    let stages: Vec<Stage> = vec![];
    let output = build_graph_display(&stages).unwrap();
    assert!(output.contains("no stages found"));
}

#[test]
fn test_build_graph_display_single_stage() {
    let stages = vec![create_test_stage(
        "stage-1",
        "First Stage",
        StageStatus::Queued,
        vec![],
    )];

    let output = build_graph_display(&stages).unwrap();
    assert!(output.contains('▶')); // Ready indicator
    assert!(output.contains("First Stage"));
    assert!(output.contains("stage-1"));
    assert!(
        output.contains("Level 0"),
        "Single stage should be at level 0"
    );
}

#[test]
fn test_build_graph_display_linear_chain() {
    let stages = vec![
        create_test_stage("stage-1", "First", StageStatus::Completed, vec![]),
        create_test_stage("stage-2", "Second", StageStatus::Executing, vec!["stage-1"]),
        create_test_stage(
            "stage-3",
            "Third",
            StageStatus::WaitingForDeps,
            vec!["stage-2"],
        ),
    ];

    let output = build_graph_display(&stages).unwrap();

    // Check that all stages appear
    assert!(output.contains("First"));
    assert!(output.contains("Second"));
    assert!(output.contains("Third"));

    // Check status indicators
    assert!(output.contains('✓')); // Completed
    assert!(output.contains('●')); // Executing
    assert!(output.contains('○')); // Pending

    // Check level structure for linear chain
    assert!(output.contains("Level 0"), "Should have level 0");
    assert!(output.contains("Level 1:"), "Should have level 1");
    assert!(output.contains("Level 2:"), "Should have level 2");

    // Second should show dependency on First
    assert!(output.contains("← "), "Should show dependency arrows");
}

#[test]
fn test_build_graph_display_diamond_pattern() {
    // Diamond: A -> B, A -> C, B -> D, C -> D
    let stages = vec![
        create_test_stage("a", "Stage A", StageStatus::Completed, vec![]),
        create_test_stage("b", "Stage B", StageStatus::Completed, vec!["a"]),
        create_test_stage("c", "Stage C", StageStatus::Completed, vec!["a"]),
        create_test_stage("d", "Stage D", StageStatus::Queued, vec!["b", "c"]),
    ];

    let output = build_graph_display(&stages).unwrap();

    // All stages should be present
    assert!(output.contains("Stage A"));
    assert!(output.contains("Stage B"));
    assert!(output.contains("Stage C"));
    assert!(output.contains("Stage D"));

    // Check level structure: A at level 0, B and C at level 1, D at level 2
    assert!(output.contains("Level 0"), "Should have level 0 header");
    assert!(output.contains("Level 1:"), "Should have level 1 header");
    assert!(output.contains("Level 2:"), "Should have level 2 header");

    // D should show ALL its dependencies (both b and c)
    assert!(
        output.contains("← ") && output.contains("b") && output.contains("c"),
        "Diamond node D should show all dependencies"
    );
}

#[test]
fn test_build_graph_display_shows_all_deps() {
    // Simulate the user's scenario: integration-tests depends on 3 stages,
    // one of which is still executing
    let stages = vec![
        create_test_stage(
            "state-machine",
            "State Machine",
            StageStatus::Completed,
            vec![],
        ),
        create_test_stage(
            "merge-completed",
            "Merge Completed",
            StageStatus::Completed,
            vec!["state-machine"],
        ),
        create_test_stage(
            "complete-refactor",
            "Complete Refactor",
            StageStatus::Executing,
            vec!["state-machine"],
        ),
        create_test_stage(
            "criteria-validation",
            "Criteria Validation",
            StageStatus::Completed,
            vec![],
        ),
        create_test_stage(
            "context-vars",
            "Context Variables",
            StageStatus::Completed,
            vec!["criteria-validation"],
        ),
        create_test_stage(
            "integration-tests",
            "Integration Tests",
            StageStatus::WaitingForDeps,
            vec!["complete-refactor", "merge-completed", "context-vars"],
        ),
    ];

    let output = build_graph_display(&stages).unwrap();

    // integration-tests should be present at level 2
    assert!(
        output.contains("integration-tests"),
        "Integration tests stage should be present"
    );

    // Should show ALL dependencies with "←" (not "← also:")
    assert!(
        output.contains("← "),
        "Multi-dep stage should show dependencies"
    );

    // The output should show all dependencies including the blocking one
    // (complete-refactor with ● indicator)
    assert!(
        output.contains("complete-refactor")
            && output.contains("merge-completed")
            && output.contains("context-vars"),
        "Should show all dependencies for integration-tests"
    );

    // Verify level structure for this complex graph
    assert!(output.contains("Level 0"), "Should have level 0");
    assert!(output.contains("Level 1:"), "Should have level 1");
    assert!(output.contains("Level 2:"), "Should have level 2");
}

#[test]
fn test_build_graph_display_multiple_roots() {
    let stages = vec![
        create_test_stage("root-1", "Root One", StageStatus::Queued, vec![]),
        create_test_stage("root-2", "Root Two", StageStatus::Executing, vec![]),
        create_test_stage(
            "child",
            "Child",
            StageStatus::WaitingForDeps,
            vec!["root-1", "root-2"],
        ),
    ];

    let output = build_graph_display(&stages).unwrap();

    // All stages present
    assert!(output.contains("Root One"));
    assert!(output.contains("Root Two"));
    assert!(output.contains("Child"));

    // Both roots at level 0, child at level 1
    assert!(output.contains("Level 0"), "Should have level 0");
    assert!(output.contains("Level 1:"), "Should have level 1");

    // Child should show both dependencies
    assert!(
        output.contains("root-1") && output.contains("root-2"),
        "Child should show both parent dependencies"
    );
}

#[test]
fn test_build_graph_display_all_statuses() {
    let stages = vec![
        create_test_stage("s1", "Completed Stage", StageStatus::Completed, vec![]),
        create_test_stage("s2", "Executing Stage", StageStatus::Executing, vec![]),
        create_test_stage("s3", "Ready Stage", StageStatus::Queued, vec![]),
        create_test_stage("s4", "Pending Stage", StageStatus::WaitingForDeps, vec![]),
        create_test_stage(
            "s5",
            "WaitingForInput Stage",
            StageStatus::WaitingForInput,
            vec![],
        ),
        create_test_stage("s6", "Blocked Stage", StageStatus::Blocked, vec![]),
        create_test_stage(
            "s7",
            "NeedsHandoff Stage",
            StageStatus::NeedsHandoff,
            vec![],
        ),
    ];

    let output = build_graph_display(&stages).unwrap();

    // Check all Unicode status indicators are present
    assert!(output.contains('✓')); // Completed
    assert!(output.contains('●')); // Executing
    assert!(output.contains('▶')); // Ready
    assert!(output.contains('○')); // Pending
    assert!(output.contains('?')); // WaitingForInput
    assert!(output.contains('✗')); // Blocked
    assert!(output.contains('⟳')); // NeedsHandoff
}

#[test]
fn test_build_graph_display_status_sorting() {
    // Stages with different statuses at same level - should be sorted by priority
    let stages = vec![
        create_test_stage("a", "Completed A", StageStatus::Completed, vec![]),
        create_test_stage("b", "Executing B", StageStatus::Executing, vec![]),
        create_test_stage("c", "Ready C", StageStatus::Queued, vec![]),
    ];

    let output = build_graph_display(&stages).unwrap();

    // Executing should appear before Ready, which should appear before Completed
    let pos_exec = output.find("Executing B").unwrap();
    let pos_ready = output.find("Ready C").unwrap();
    let pos_completed = output.find("Completed A").unwrap();

    assert!(
        pos_exec < pos_ready && pos_ready < pos_completed,
        "Stages should be sorted by status priority: executing < ready < completed"
    );
}

#[test]
fn test_compute_stage_levels() {
    let stages = vec![
        create_test_stage("a", "A", StageStatus::Completed, vec![]),
        create_test_stage("b", "B", StageStatus::Completed, vec!["a"]),
        create_test_stage("c", "C", StageStatus::Completed, vec!["a"]),
        create_test_stage("d", "D", StageStatus::Completed, vec!["b", "c"]),
    ];

    let levels = compute_stage_levels(&stages);

    assert_eq!(levels.get("a"), Some(&0));
    assert_eq!(levels.get("b"), Some(&1));
    assert_eq!(levels.get("c"), Some(&1));
    assert_eq!(levels.get("d"), Some(&2));
}

// ============================================================================
// Tree Display Tests
// ============================================================================

#[test]
fn test_tree_display_empty() {
    let stages: Vec<Stage> = vec![];
    let output = build_tree_display(&stages);
    assert!(
        output.to_lowercase().contains("no stages"),
        "Empty stages should show 'no stages' message"
    );
}

#[test]
fn test_tree_display_single_stage() {
    let stages = vec![create_test_stage(
        "init",
        "Initialize",
        StageStatus::Completed,
        vec![],
    )];

    let output = build_tree_display(&stages);
    assert!(output.contains("init"), "Should contain stage id");
    assert!(
        !output.contains("├──") && !output.contains("└──"),
        "Root stage should have no tree connector"
    );
}

#[test]
fn test_tree_display_linear_chain() {
    let stages = vec![
        create_test_stage("a", "Stage A", StageStatus::Completed, vec![]),
        create_test_stage("b", "Stage B", StageStatus::Completed, vec!["a"]),
        create_test_stage("c", "Stage C", StageStatus::Queued, vec!["b"]),
    ];

    let output = build_tree_display(&stages);

    // Should contain stage IDs (not names)
    assert!(output.contains("✓ a"), "Should contain stage a");
    assert!(output.contains("✓ b"), "Should contain stage b");
    assert!(output.contains("▶ c"), "Should contain stage c");
    assert!(
        output.contains("← a"),
        "Stage b should show dependency on a"
    );
    assert!(
        output.contains("← b"),
        "Stage c should show dependency on b"
    );
}

#[test]
fn test_tree_display_diamond_pattern() {
    let stages = vec![
        create_test_stage("a", "Root", StageStatus::Completed, vec![]),
        create_test_stage("b", "Left", StageStatus::Completed, vec!["a"]),
        create_test_stage("c", "Right", StageStatus::Completed, vec!["a"]),
        create_test_stage("d", "Merge", StageStatus::Queued, vec!["b", "c"]),
    ];

    let output = build_tree_display(&stages);

    // Should contain stage IDs
    assert!(output.contains("a"), "Should contain stage a");
    assert!(output.contains("b"), "Should contain stage b");
    assert!(output.contains("c"), "Should contain stage c");
    assert!(output.contains("d"), "Should contain stage d");
    // Stage d should show both dependencies
    assert!(
        output.contains("← b, c") || output.contains("← c, b"),
        "Stage d should show both b and c in dependency annotation"
    );
}

#[test]
fn test_tree_connector_last_item() {
    let stages = vec![
        create_test_stage("a", "First", StageStatus::Completed, vec![]),
        create_test_stage("b", "Second", StageStatus::Completed, vec!["a"]),
        create_test_stage("c", "Third", StageStatus::Completed, vec!["b"]),
    ];

    let output = build_tree_display(&stages);

    assert!(
        output.contains("└──"),
        "Last item in tree should use └── connector"
    );
    assert!(
        !output.ends_with("├──"),
        "Tree should not end with ├── connector"
    );
}
