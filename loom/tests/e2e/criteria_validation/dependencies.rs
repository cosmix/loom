//! Dependency validation tests
//!
//! Tests for dependency graph validation including unknown dependencies,
//! self-references, and path traversal prevention.

use super::{create_metadata, create_valid_stage};
use loom::plan::schema::validate;

/// Test that valid dependencies are accepted
#[test]
fn test_valid_dependencies() {
    let stage1 = create_valid_stage("stage-1", "Stage One");
    let mut stage2 = create_valid_stage("stage-2", "Stage Two");
    stage2.dependencies.push("stage-1".to_string());

    let metadata = create_metadata(vec![stage1, stage2]);
    let result = validate(&metadata);

    assert!(result.is_ok(), "Valid dependencies should be accepted");
}

/// Test that unknown dependencies are rejected
#[test]
fn test_unknown_dependency_rejected() {
    let mut stage = create_valid_stage("stage-1", "Stage One");
    stage.dependencies.push("nonexistent-stage".to_string());

    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Unknown dependency should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Unknown dependency")));
}

/// Test that self-dependency is rejected
#[test]
fn test_self_dependency_rejected() {
    let mut stage = create_valid_stage("stage-1", "Stage One");
    stage.dependencies.push("stage-1".to_string());

    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Self-dependency should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("cannot depend on itself")));
}

/// Test that path traversal in dependency IDs is rejected
#[test]
fn test_dependency_id_path_traversal_rejected() {
    let mut stage = create_valid_stage("stage-1", "Stage One");
    stage.dependencies.push("../etc/passwd".to_string());

    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(
        result.is_err(),
        "Path traversal in dependency should be rejected"
    );
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Invalid dependency ID")));
}
