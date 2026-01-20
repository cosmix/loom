//! Stage ID validation tests

use crate::plan::schema::types::{LoomConfig, LoomMetadata, StageDefinition, StageType};
use crate::plan::schema::validation::validate;

#[test]
fn test_validate_stage_id_path_traversal() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "../etc/passwd".to_string(),
                name: "Malicious Stage".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
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
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage/with/slashes".to_string(),
                name: "Stage".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
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
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage.with.dots".to_string(),
                name: "Stage".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
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
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "..".to_string(),
                name: "Stage".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
}

#[test]
fn test_validate_stage_id_reserved_name_con() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "CON".to_string(),
                name: "Stage".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("reserved name")));
}

#[test]
fn test_validate_dependency_id_path_traversal() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage-1".to_string(),
                name: "Stage One".to_string(),
                description: None,
                dependencies: vec!["../etc/passwd".to_string()],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
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
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: long_id,
                name: "Stage".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("too long")));
}

#[test]
fn test_validate_stage_id_with_spaces() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage with spaces".to_string(),
                name: "Stage".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("invalid characters")));
}
