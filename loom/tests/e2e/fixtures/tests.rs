//! Tests for plan fixtures
//!
//! Validates that all fixture generators produce valid loom plan content

use super::plans::*;
use loom::plan::parser::parse_plan_content;
use std::path::PathBuf;

#[test]
fn test_simple_sequential_plan_is_valid() {
    let content = simple_sequential_plan();
    let path = PathBuf::from("test-plan.md");

    let parsed = parse_plan_content(&content, &path).expect("Should parse simple sequential plan");

    assert_eq!(parsed.name, "Simple Sequential Test");
    assert_eq!(parsed.stages.len(), 2);
    assert_eq!(parsed.stages[0].id, "stage-1");
    assert_eq!(parsed.stages[1].id, "stage-2");
    assert_eq!(parsed.stages[1].dependencies, vec!["stage-1"]);
}

#[test]
fn test_parallel_plan_is_valid() {
    let content = parallel_plan();
    let path = PathBuf::from("test-plan.md");

    let parsed = parse_plan_content(&content, &path).expect("Should parse parallel plan");

    assert_eq!(parsed.name, "Parallel Execution Test");
    assert_eq!(parsed.stages.len(), 3);
    assert_eq!(parsed.stages[0].id, "stage-1");
    assert_eq!(parsed.stages[1].id, "stage-2");
    assert_eq!(parsed.stages[2].id, "stage-3");

    assert_eq!(
        parsed.stages[1].parallel_group,
        Some("parallel-group-1".to_string())
    );
    assert_eq!(
        parsed.stages[2].parallel_group,
        Some("parallel-group-1".to_string())
    );

    assert_eq!(parsed.stages[1].dependencies, vec!["stage-1"]);
    assert_eq!(parsed.stages[2].dependencies, vec!["stage-1"]);
}

#[test]
fn test_complex_plan_is_valid() {
    let content = complex_plan();
    let path = PathBuf::from("test-plan.md");

    let parsed = parse_plan_content(&content, &path).expect("Should parse complex plan");

    assert_eq!(parsed.name, "Complex Dependencies Test");
    assert_eq!(parsed.stages.len(), 4);

    assert_eq!(parsed.stages[0].dependencies.len(), 0);
    assert_eq!(parsed.stages[1].dependencies, vec!["stage-1"]);
    assert_eq!(parsed.stages[2].dependencies, vec!["stage-1"]);
    assert_eq!(parsed.stages[3].dependencies, vec!["stage-2", "stage-3"]);
}

#[test]
fn test_stage_with_acceptance_is_valid() {
    let content = stage_with_acceptance();
    let path = PathBuf::from("test-plan.md");

    let parsed = parse_plan_content(&content, &path).expect("Should parse acceptance plan");

    assert_eq!(parsed.name, "Acceptance Criteria Test");
    assert_eq!(parsed.stages.len(), 1);
    assert_eq!(parsed.stages[0].acceptance.len(), 4);
    assert_eq!(parsed.stages[0].files.len(), 3);
}

#[test]
fn test_minimal_plan_is_valid() {
    let content = minimal_plan();
    let path = PathBuf::from("test-plan.md");

    let parsed = parse_plan_content(&content, &path).expect("Should parse minimal plan");

    assert_eq!(parsed.name, "Minimal Test");
    assert_eq!(parsed.stages.len(), 1);
    assert_eq!(parsed.stages[0].id, "stage-1");
    assert_eq!(parsed.stages[0].name, "Minimal Stage");
}

#[test]
fn test_long_sequential_chain_is_valid() {
    let content = long_sequential_chain();
    let path = PathBuf::from("test-plan.md");

    let parsed = parse_plan_content(&content, &path).expect("Should parse long chain plan");

    assert_eq!(parsed.name, "Long Sequential Chain");
    assert_eq!(parsed.stages.len(), 5);

    assert_eq!(parsed.stages[0].dependencies.len(), 0);
    assert_eq!(parsed.stages[1].dependencies, vec!["stage-1"]);
    assert_eq!(parsed.stages[2].dependencies, vec!["stage-2"]);
    assert_eq!(parsed.stages[3].dependencies, vec!["stage-3"]);
    assert_eq!(parsed.stages[4].dependencies, vec!["stage-4"]);
}

#[test]
fn test_all_fixtures_parse_successfully() {
    let path = PathBuf::from("test.md");

    let fixtures = vec![
        ("simple_sequential", simple_sequential_plan()),
        ("parallel", parallel_plan()),
        ("complex", complex_plan()),
        ("acceptance", stage_with_acceptance()),
        ("minimal", minimal_plan()),
        ("long_chain", long_sequential_chain()),
    ];

    for (name, content) in fixtures {
        let result = parse_plan_content(&content, &path);
        assert!(
            result.is_ok(),
            "Fixture '{}' failed to parse: {:?}",
            name,
            result.err()
        );
    }
}
