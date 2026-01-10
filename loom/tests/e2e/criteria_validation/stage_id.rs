//! Stage ID validation tests
//!
//! Tests for stage ID format, path traversal prevention, and reserved name handling.

use super::{create_metadata, create_valid_stage};
use loom::plan::schema::validate;

/// Test that valid stage IDs are accepted
#[test]
fn test_valid_stage_ids() {
    let valid_ids = vec![
        "stage-1",
        "my-stage",
        "stage_with_underscores",
        "CamelCase",
        "mix-of_both",
        "a",
        "123",
        "a-b-c",
    ];

    for id in valid_ids {
        let stage = create_valid_stage(id, "Test Stage");
        let metadata = create_metadata(vec![stage]);
        let result = validate(&metadata);
        assert!(
            result.is_ok(),
            "Stage ID '{}' should be valid, got: {:?}",
            id,
            result.err()
        );
    }
}

/// Test that path traversal attempts in stage IDs are rejected
#[test]
fn test_stage_id_path_traversal_rejected() {
    let malicious_ids = vec![
        "../etc/passwd",
        "..\\windows\\system32",
        "foo/../bar",
        "./hidden",
        "stage/../../../etc/shadow",
    ];

    for id in malicious_ids {
        let stage = create_valid_stage(id, "Test Stage");
        let metadata = create_metadata(vec![stage]);
        let result = validate(&metadata);
        assert!(
            result.is_err(),
            "Path traversal ID '{id}' should be rejected"
        );

        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("Invalid stage ID")),
            "Error should mention Invalid stage ID for '{id}'"
        );
    }
}

/// Test that stage IDs with slashes are rejected
#[test]
fn test_stage_id_with_slashes_rejected() {
    let stage = create_valid_stage("stage/with/slashes", "Test Stage");
    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Stage ID with slashes should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("invalid characters")));
}

/// Test that stage IDs with dots are rejected
#[test]
fn test_stage_id_with_dots_rejected() {
    let stage = create_valid_stage("stage.with.dots", "Test Stage");
    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Stage ID with dots should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("invalid characters")));
}

/// Test that reserved names are rejected
#[test]
fn test_reserved_stage_ids_rejected() {
    let reserved_names = vec!["CON", "PRN", "AUX", "NUL", "COM1", "LPT1", "..", "."];

    for name in reserved_names {
        let stage = create_valid_stage(name, "Test Stage");
        let metadata = create_metadata(vec![stage]);
        let result = validate(&metadata);
        assert!(result.is_err(), "Reserved name '{name}' should be rejected");
    }
}

/// Test that stage IDs with spaces are rejected
#[test]
fn test_stage_id_with_spaces_rejected() {
    let stage = create_valid_stage("stage with spaces", "Test Stage");
    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Stage ID with spaces should be rejected");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("invalid characters")));
}

/// Test that stage IDs that are too long are rejected
#[test]
fn test_stage_id_too_long_rejected() {
    let long_id = "a".repeat(129);
    let stage = create_valid_stage(&long_id, "Test Stage");
    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(
        result.is_err(),
        "Stage ID over 128 chars should be rejected"
    );
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("too long")));
}

/// Test that empty stage ID is rejected
#[test]
fn test_empty_stage_id_rejected() {
    let stage = create_valid_stage("", "Test Stage");
    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Empty stage ID should be rejected");
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("cannot be empty")));
}
