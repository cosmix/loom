//! Acceptance criteria validation tests
//!
//! Tests for acceptance criterion content, length, and control character handling.

use super::{create_metadata, create_valid_stage};
use loom::plan::schema::validate;

/// Test that valid acceptance criteria are accepted
#[test]
fn test_valid_acceptance_criteria() {
    let valid_criteria = vec![
        "cargo test",
        "cargo build --release",
        "npm run test && npm run lint",
        "cd ${PROJECT_ROOT} && cargo test --lib",
        "pytest tests/ -v",
        "make all",
        "go test ./...",
    ];

    for criterion in valid_criteria {
        let mut stage = create_valid_stage("test-stage", "Test");
        stage.acceptance.push(criterion.to_string());
        let metadata = create_metadata(vec![stage]);
        let result = validate(&metadata);
        assert!(
            result.is_ok(),
            "Criterion '{}' should be valid, got: {:?}",
            criterion,
            result.err()
        );
    }
}

/// Test that empty acceptance criteria are rejected
#[test]
fn test_empty_acceptance_criterion_rejected() {
    let mut stage = create_valid_stage("test-stage", "Test Stage");
    stage.acceptance.push("".to_string());

    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(
        result.is_err(),
        "Empty acceptance criterion should be rejected"
    );
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("empty")));
}

/// Test that whitespace-only acceptance criteria are rejected
#[test]
fn test_whitespace_only_acceptance_criterion_rejected() {
    let whitespace_criteria = vec!["   ", "\t", "\n", "  \t  \n  "];

    for criterion in whitespace_criteria {
        let mut stage = create_valid_stage("test-stage", "Test Stage");
        stage.acceptance.push(criterion.to_string());

        let metadata = create_metadata(vec![stage]);
        let result = validate(&metadata);

        assert!(
            result.is_err(),
            "Whitespace-only criterion '{criterion:?}' should be rejected"
        );
    }
}

/// Test that acceptance criteria with control characters are rejected
#[test]
fn test_control_chars_in_acceptance_rejected() {
    let control_chars = vec![
        ("null byte", "cargo\x00test"),
        ("bell", "cargo\x07test"),
        ("backspace", "cargo\x08test"),
        ("form feed", "cargo\x0Ctest"),
        ("escape", "cargo\x1Btest"),
    ];

    for (name, criterion) in control_chars {
        let mut stage = create_valid_stage("test-stage", "Test Stage");
        stage.acceptance.push(criterion.to_string());

        let metadata = create_metadata(vec![stage]);
        let result = validate(&metadata);

        assert!(result.is_err(), "Criterion with {name} should be rejected");
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("control character")),
            "Error should mention control character for {name}"
        );
    }
}

/// Test that tabs and newlines are allowed in acceptance criteria
#[test]
fn test_allowed_whitespace_in_acceptance() {
    let allowed_whitespace = vec![
        "cargo test\t--lib",
        "cargo test\n",
        "cargo build\r\n",
        "echo 'hello'\tworld",
    ];

    for criterion in allowed_whitespace {
        let mut stage = create_valid_stage("test-stage", "Test Stage");
        stage.acceptance.push(criterion.to_string());

        let metadata = create_metadata(vec![stage]);
        let result = validate(&metadata);

        assert!(
            result.is_ok(),
            "Criterion with standard whitespace '{}' should be allowed, got: {:?}",
            criterion.escape_debug(),
            result.err()
        );
    }
}

/// Test that acceptance criteria over 1024 chars are rejected
#[test]
fn test_acceptance_criterion_too_long_rejected() {
    let long_criterion = "a".repeat(1025);

    let mut stage = create_valid_stage("test-stage", "Test Stage");
    stage.acceptance.push(long_criterion);

    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(
        result.is_err(),
        "Criterion over 1024 chars should be rejected"
    );
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("too long")));
}

/// Test that multiple invalid acceptance criteria generate multiple errors
#[test]
fn test_multiple_invalid_acceptance_criteria() {
    let mut stage = create_valid_stage("test-stage", "Test Stage");
    stage.acceptance.push("".to_string());
    stage.acceptance.push("   ".to_string());
    stage.acceptance.push("valid command".to_string());
    stage.acceptance.push("\t\n".to_string());

    let metadata = create_metadata(vec![stage]);
    let result = validate(&metadata);

    assert!(result.is_err(), "Should detect multiple invalid criteria");
    let errors = result.unwrap_err();

    let acceptance_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("acceptance criterion"))
        .collect();
    assert_eq!(
        acceptance_errors.len(),
        3,
        "Should have 3 acceptance criterion errors, got: {errors:?}"
    );
}
