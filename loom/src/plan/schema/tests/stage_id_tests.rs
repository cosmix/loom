//! Stage ID validation tests

use super::make_stage;
use crate::plan::schema::types::{LoomConfig, LoomMetadata, SandboxConfig};
use crate::plan::schema::validation::validate;

#[test]
fn test_validate_stage_id_path_traversal() {
    let stage = make_stage("../etc/passwd", "Malicious Stage");

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
        .any(|e| e.message.contains("Invalid stage ID")));
}

#[test]
fn test_validate_stage_id_with_slashes() {
    let stage = make_stage("stage/with/slashes", "Stage");

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
        .any(|e| e.message.contains("invalid characters")));
}

#[test]
fn test_validate_stage_id_with_dots() {
    let stage = make_stage("stage.with.dots", "Stage");

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
        .any(|e| e.message.contains("invalid characters")));
}

#[test]
fn test_validate_stage_id_reserved_name_dotdot() {
    let stage = make_stage("..", "Stage");

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
}

#[test]
fn test_validate_stage_id_reserved_name_con() {
    let stage = make_stage("CON", "Stage");

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
    assert!(errors.iter().any(|e| e.message.contains("reserved name")));
}

#[test]
fn test_validate_dependency_id_path_traversal() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.dependencies = vec!["../etc/passwd".to_string()];

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
        .any(|e| e.message.contains("Invalid dependency ID")));
}

#[test]
fn test_validate_stage_id_too_long() {
    let long_id = "a".repeat(129);
    let stage = make_stage(&long_id, "Stage");

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
    assert!(errors.iter().any(|e| e.message.contains("too long")));
}

#[test]
fn test_validate_stage_id_with_spaces() {
    let stage = make_stage("stage with spaces", "Stage");

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
        .any(|e| e.message.contains("invalid characters")));
}
