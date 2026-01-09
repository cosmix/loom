//! Integration tests for acceptance criteria validation
//!
//! These tests verify that acceptance criteria are validated at plan init time,
//! preventing invalid criteria from being used in plans.

use loom::plan::schema::{validate, LoomConfig, LoomMetadata, StageDefinition, ValidationError};

/// Helper to create a minimal valid stage definition
fn create_valid_stage(id: &str, name: &str) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
    }
}

/// Helper to create minimal valid metadata with given stages
fn create_metadata(stages: Vec<StageDefinition>) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig { version: 1, stages },
    }
}

// ============================================================================
// Stage ID Validation Tests
// ============================================================================

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
            errors.iter().any(|e| e.message.contains("Invalid stage ID")),
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
    // Windows reserved device names
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

    assert!(result.is_err(), "Stage ID over 128 chars should be rejected");
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
    assert!(errors
        .iter()
        .any(|e| e.message.contains("cannot be empty")));
}

// ============================================================================
// Acceptance Criteria Validation Tests
// ============================================================================

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
    // Control characters that should be rejected
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
            errors.iter().any(|e| e.message.contains("control character")),
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

    assert!(result.is_err(), "Criterion over 1024 chars should be rejected");
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

    // Should have 3 errors (empty, whitespace-only x2)
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

// ============================================================================
// Dependency Validation Tests
// ============================================================================

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

// ============================================================================
// Plan Structure Validation Tests
// ============================================================================

/// Test that unsupported version is rejected
#[test]
fn test_unsupported_version_rejected() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 2,
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
            version: 2, // Invalid version
            stages: vec![
                create_valid_stage("", ""), // Empty ID and name
                {
                    let mut s = create_valid_stage("stage-2", "Stage Two");
                    s.dependencies.push("nonexistent".to_string()); // Unknown dep
                    s.dependencies.push("stage-2".to_string()); // Self dep
                    s.acceptance.push("".to_string()); // Empty criterion
                    s
                },
            ],
        },
    };

    let result = validate(&metadata);

    assert!(result.is_err(), "Should detect multiple errors");
    let errors = result.unwrap_err();

    // Should have at least: unsupported version, empty ID, empty name,
    // unknown dependency, self-dependency, empty acceptance
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
