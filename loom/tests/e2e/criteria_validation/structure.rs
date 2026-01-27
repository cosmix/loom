//! Plan structure validation tests
//!
//! Tests for plan metadata, version validation, stage structure,
//! and error accumulation.

use super::{create_metadata, create_valid_stage};
use loom::plan::schema::{validate, LoomConfig, LoomMetadata, ValidationError};

/// Test that unsupported version is rejected
#[test]
fn test_unsupported_version_rejected() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 2,
            sandbox: Default::default(),
            auto_merge: None,
            stages: vec![create_valid_stage("stage-1", "Test")],
        },
    };

    let result = validate(&metadata);

    assert!(result.is_err(), "Unsupported version should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Unsupported version")));
}

/// Test that empty stages list is rejected
#[test]
fn test_empty_stages_rejected() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            sandbox: Default::default(),
            auto_merge: None,
            stages: vec![],
        },
    };

    let result = validate(&metadata);

    assert!(result.is_err(), "Empty stages should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("No stages defined")));
}

/// Test that empty stage name is rejected
#[test]
fn test_empty_stage_name_rejected() {
    let stage = create_valid_stage("stage-1", "");

    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Empty stage name should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("name cannot be empty")));
}

/// Test that complex dependency chains are validated
#[test]
fn test_complex_dependency_chain_validated() {
    let stage1 = create_valid_stage("stage-1", "Stage One");

    let mut stage2 = create_valid_stage("stage-2", "Stage Two");
    stage2.dependencies.push("stage-1".to_string());

    let mut stage3 = create_valid_stage("stage-3", "Stage Three");
    stage3.dependencies.push("stage-1".to_string());
    stage3.dependencies.push("stage-2".to_string());

    let metadata = create_metadata(vec![stage1, stage2, stage3]);
    let result = validate(&metadata);

    assert!(
        result.is_ok(),
        "Complex dependency chain should be valid: {:?}",
        result.err()
    );
}

/// Test that multiple errors are accumulated
#[test]
fn test_multiple_errors_accumulated() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 2,
            sandbox: Default::default(),
            auto_merge: None,
            stages: vec![create_valid_stage("", ""), {
                let mut s = create_valid_stage("stage-2", "Stage Two");
                s.dependencies.push("nonexistent".to_string());
                s.dependencies.push("stage-2".to_string());
                s.acceptance.push("".to_string());
                s
            }],
        },
    };

    let result = validate(&metadata);

    assert!(result.is_err(), "Should detect multiple errors");
    let errors = result.unwrap_err();

    assert!(
        errors.len() >= 5,
        "Should have multiple errors, got {} errors: {:?}",
        errors.len(),
        errors
    );
}

/// Test ValidationError display formatting
#[test]
fn test_validation_error_display() {
    let error_with_stage = ValidationError {
        message: "Test error message".to_string(),
        stage_id: Some("my-stage".to_string()),
    };
    assert_eq!(
        error_with_stage.to_string(),
        "Stage 'my-stage': Test error message"
    );

    let error_without_stage = ValidationError {
        message: "General error".to_string(),
        stage_id: None,
    };
    assert_eq!(error_without_stage.to_string(), "General error");
}
