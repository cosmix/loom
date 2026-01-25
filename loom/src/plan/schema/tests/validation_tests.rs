//! Basic validation tests

use super::create_valid_metadata;
use crate::plan::schema::types::{
    LoomConfig, LoomMetadata, StageDefinition, StageType, ValidationError,
};
use crate::plan::schema::validation::validate;

#[test]
fn test_validate_valid_metadata() {
    let metadata = create_valid_metadata();
    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_validate_unsupported_version() {
    let mut metadata = create_valid_metadata();
    metadata.loom.version = 2;

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("Unsupported version"));
}

#[test]
fn test_validate_empty_stages() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("No stages defined")));
}

#[test]
fn test_validate_empty_stage_id() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "".to_string(),
                name: "Test".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("ID cannot be empty")));
}

#[test]
fn test_validate_empty_stage_name() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage-1".to_string(),
                name: "".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("name cannot be empty")));
}

#[test]
fn test_validate_unknown_dependency() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage-1".to_string(),
                name: "Stage One".to_string(),
                description: None,
                dependencies: vec!["nonexistent".to_string()],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Unknown dependency")));
    assert!(errors.iter().any(|e| e.message.contains("nonexistent")));
}

#[test]
fn test_validate_self_dependency() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage-1".to_string(),
                name: "Stage One".to_string(),
                description: None,
                dependencies: vec!["stage-1".to_string()],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("cannot depend on itself")));
}

#[test]
fn test_validate_multiple_errors() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 2,
            auto_merge: None,
            stages: vec![
                StageDefinition {
                    id: "".to_string(),
                    name: "".to_string(),
                    description: None,
                    dependencies: vec![],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                },
                StageDefinition {
                    id: "stage-2".to_string(),
                    name: "Stage Two".to_string(),
                    description: None,
                    dependencies: vec!["stage-2".to_string(), "nonexistent".to_string()],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                },
            ],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    // Should have multiple errors: unsupported version, empty ID, empty name, self-dependency, unknown dependency
    assert!(errors.len() >= 4);
}

#[test]
fn test_validation_error_display() {
    let error = ValidationError {
        message: "Test error".to_string(),
        stage_id: Some("stage-1".to_string()),
    };
    assert_eq!(error.to_string(), "Stage 'stage-1': Test error");

    let error_no_stage = ValidationError {
        message: "General error".to_string(),
        stage_id: None,
    };
    assert_eq!(error_no_stage.to_string(), "General error");
}

#[test]
fn test_stage_definition_serde_defaults() {
    let yaml = r#"
id: test-stage
name: Test Stage
working_dir: "."
"#;
    let stage: StageDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(stage.id, "test-stage");
    assert_eq!(stage.name, "Test Stage");
    assert_eq!(stage.description, None);
    assert_eq!(stage.dependencies.len(), 0);
    assert_eq!(stage.parallel_group, None);
    assert_eq!(stage.acceptance.len(), 0);
    assert_eq!(stage.setup.len(), 0);
    assert_eq!(stage.files.len(), 0);
    assert_eq!(stage.auto_merge, None);
    assert_eq!(stage.working_dir, ".");
}

#[test]
fn test_complex_dependency_chain() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![
                StageDefinition {
                    id: "stage-1".to_string(),
                    name: "Stage 1".to_string(),
                    description: None,
                    dependencies: vec![],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                },
                StageDefinition {
                    id: "stage-2".to_string(),
                    name: "Stage 2".to_string(),
                    description: None,
                    dependencies: vec!["stage-1".to_string()],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                },
                StageDefinition {
                    id: "stage-3".to_string(),
                    name: "Stage 3".to_string(),
                    description: None,
                    dependencies: vec!["stage-1".to_string(), "stage-2".to_string()],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                },
            ],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_validate_duplicate_stage_ids() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![
                StageDefinition {
                    id: "stage-1".to_string(),
                    name: "Stage One".to_string(),
                    description: None,
                    dependencies: vec![],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                },
                StageDefinition {
                    id: "stage-1".to_string(), // Duplicate ID
                    name: "Stage One Duplicate".to_string(),
                    description: None,
                    dependencies: vec![],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                },
            ],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Duplicate stage IDs")));
}

#[test]
fn test_validate_working_dir_path_traversal() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage-1".to_string(),
                name: "Stage One".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: "../etc".to_string(), // Path traversal
                stage_type: StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("path traversal")));
}

#[test]
fn test_validate_working_dir_absolute_path() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage-1".to_string(),
                name: "Stage One".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: "/etc/passwd".to_string(), // Absolute path
                stage_type: StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("must be relative path")));
}

#[test]
fn test_validate_working_dir_valid_subdirectory() {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages: vec![StageDefinition {
                id: "stage-1".to_string(),
                name: "Stage One".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: "loom".to_string(), // Valid subdirectory
                stage_type: StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
            }],
        },
    };

    assert!(validate(&metadata).is_ok());
}
