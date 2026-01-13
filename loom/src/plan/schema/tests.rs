//! Tests for plan YAML schema

use super::types::{LoomConfig, LoomMetadata, StageDefinition, StageType, ValidationError};
use super::validation::{check_knowledge_recommendations, validate, validate_acceptance_criterion};

fn create_valid_metadata() -> LoomMetadata {
    LoomMetadata {
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
                },
                StageDefinition {
                    id: "stage-2".to_string(),
                    name: "Stage Two".to_string(),
                    description: Some("Second stage".to_string()),
                    dependencies: vec!["stage-1".to_string()],
                    parallel_group: Some("group-a".to_string()),
                    acceptance: vec!["cargo test".to_string()],
                    setup: vec!["source .venv/bin/activate".to_string()],
                    files: vec!["src/*.rs".to_string()],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::default(),
                },
            ],
        },
    }
}

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
                },
            ],
        },
    };

    assert!(validate(&metadata).is_ok());
}

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
                acceptance: vec!["".to_string()],
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
        .any(|e| e.message.contains("acceptance criterion")));
    assert!(errors.iter().any(|e| e.message.contains("empty")));
}

#[test]
fn test_validate_metadata_with_valid_acceptance() {
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
                acceptance: vec![
                    "cargo test".to_string(),
                    "cargo clippy -- -D warnings".to_string(),
                ],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::default(),
            }],
        },
    };

    let result = validate(&metadata);
    assert!(result.is_ok());
}

#[test]
fn test_validate_metadata_multiple_invalid_acceptance() {
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
                acceptance: vec!["".to_string(), "   ".to_string(), "cargo test".to_string()],
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
    // Should have 2 errors for the 2 invalid criteria
    let acceptance_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("acceptance criterion"))
        .collect();
    assert_eq!(acceptance_errors.len(), 2);
}

#[test]
fn test_parse_auto_merge_plan_level() {
    let yaml = r#"
loom:
  version: 1
  auto_merge: true
  stages:
    - id: stage-1
      name: "Test Stage"
      working_dir: "."
"#;
    let metadata: LoomMetadata = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metadata.loom.auto_merge, Some(true));
}

#[test]
fn test_parse_auto_merge_stage_level() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Test Stage"
      auto_merge: false
      working_dir: "."
"#;
    let metadata: LoomMetadata = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metadata.loom.stages[0].auto_merge, Some(false));
}

#[test]
fn test_auto_merge_defaults_to_none() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Test Stage"
      working_dir: "."
"#;
    let metadata: LoomMetadata = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metadata.loom.auto_merge, None);
    assert_eq!(metadata.loom.stages[0].auto_merge, None);
}

// Tests for check_knowledge_recommendations

#[test]
fn test_knowledge_recommendations_no_knowledge_stage() {
    let stages = vec![
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
        },
        StageDefinition {
            id: "stage-2".to_string(),
            name: "Stage Two".to_string(),
            description: None,
            dependencies: vec!["stage-1".to_string()],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
        },
    ];

    let warnings = check_knowledge_recommendations(&stages);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("knowledge-bootstrap"));
}

#[test]
fn test_knowledge_recommendations_has_knowledge_id() {
    let stages = vec![
        StageDefinition {
            id: "knowledge-bootstrap".to_string(),
            name: "Bootstrap".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
        },
        StageDefinition {
            id: "stage-2".to_string(),
            name: "Stage Two".to_string(),
            description: None,
            dependencies: vec!["knowledge-bootstrap".to_string()],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
        },
    ];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_has_knowledge_name() {
    let stages = vec![
        StageDefinition {
            id: "init-stage".to_string(),
            name: "Knowledge Bootstrap".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
        },
    ];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_case_insensitive() {
    let stages = vec![
        StageDefinition {
            id: "KNOWLEDGE-setup".to_string(),
            name: "Setup".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
        },
    ];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_no_root_stages() {
    // This scenario shouldn't happen in practice (plans need at least one root),
    // but if all stages have dependencies, no warning should be shown
    let stages = vec![StageDefinition {
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
    }];

    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_knowledge_recommendations_empty_stages() {
    let stages: Vec<StageDefinition> = vec![];
    let warnings = check_knowledge_recommendations(&stages);
    assert!(warnings.is_empty());
}
