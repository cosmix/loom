//! Tests for graph display functionality

use crate::models::stage::{Stage, StageStatus};

use super::indicators::status_indicator;
use super::levels::compute_stage_levels;
use super::tree::build_tree_display;

/// Strip ANSI escape codes from a string for reliable test assertions
fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else if c == '\x1b' {
            in_escape = true;
        } else {
            result.push(c);
        }
    }
    result
}

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
        !output.contains("├─") && !output.contains("└─"),
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

    let output = strip_ansi(&build_tree_display(&stages));

    // Two-space gap between status indicator and stage id (Option E layout).
    assert!(output.contains("✓  a"), "Should contain stage a");
    assert!(output.contains("✓  b"), "Should contain stage b");
    assert!(output.contains("▶  c"), "Should contain stage c");
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

    let output = strip_ansi(&build_tree_display(&stages));

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
        output.contains("└─"),
        "Last item in tree should use └─ connector"
    );
    assert!(
        !output.ends_with("├─"),
        "Tree should not end with ├─ connector"
    );
}
