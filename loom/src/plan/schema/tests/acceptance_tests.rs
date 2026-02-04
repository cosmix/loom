//! Acceptance criterion validation tests

use super::make_stage;
use crate::plan::schema::types::{LoomConfig, LoomMetadata, SandboxConfig};
use crate::plan::schema::validation::{validate, validate_acceptance_criterion};

#[test]
fn test_validate_acceptance_criterion_valid() {
    assert!(validate_acceptance_criterion("cargo test").is_ok());
    assert!(validate_acceptance_criterion("cargo build --release").is_ok());
    assert!(validate_acceptance_criterion("npm run test && npm run lint").is_ok());
    assert!(validate_acceptance_criterion("cd loom && cargo test --lib").is_ok());
}

#[test]
fn test_validate_acceptance_criterion_empty() {
    assert!(validate_acceptance_criterion("").is_err());
    assert!(validate_acceptance_criterion("   ").is_err());
    assert!(validate_acceptance_criterion("\t\n").is_err());
}

#[test]
fn test_validate_acceptance_criterion_too_long() {
    let long_criterion = "a".repeat(1025);
    let result = validate_acceptance_criterion(&long_criterion);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("too long"));
}

#[test]
fn test_validate_acceptance_criterion_control_chars() {
    // Null byte
    let result = validate_acceptance_criterion("cargo\x00test");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("control character"));

    // Bell character
    let result = validate_acceptance_criterion("cargo\x07test");
    assert!(result.is_err());
}

#[test]
fn test_validate_acceptance_criterion_allowed_whitespace() {
    // Tab, newline, carriage return should be allowed
    assert!(validate_acceptance_criterion("cargo test\t--lib").is_ok());
    assert!(validate_acceptance_criterion("cargo test\n").is_ok());
}

#[test]
fn test_validate_metadata_with_empty_acceptance() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.acceptance = vec!["".to_string()];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("acceptance criterion")));
    assert!(errors.iter().any(|e| e.message.contains("empty")));
}

#[test]
fn test_validate_metadata_with_valid_acceptance() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.acceptance = vec![
        "cargo test".to_string(),
        "cargo clippy -- -D warnings".to_string(),
    ];
    stage.truths = vec!["cargo build".to_string()];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_ok());
}

#[test]
fn test_validate_metadata_multiple_invalid_acceptance() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.acceptance = vec!["".to_string(), "   ".to_string(), "cargo test".to_string()];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    // Should have 2 errors for the 2 invalid criteria
    let acceptance_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("acceptance criterion"))
        .collect();
    assert_eq!(acceptance_errors.len(), 2);
}
