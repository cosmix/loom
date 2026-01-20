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
fn test_all_fixtures_parse_successfully() {
    let path = PathBuf::from("test.md");

    let fixtures = vec![
        ("simple_sequential", simple_sequential_plan()),
        ("minimal", minimal_plan()),
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
