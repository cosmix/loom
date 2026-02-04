//! Basic validation tests

use super::{create_valid_metadata, make_stage};
use crate::models::stage::WiringCheck;
use crate::plan::schema::types::{
    LoomConfig, LoomMetadata, SandboxConfig, StageDefinition, StageType, ValidationError,
    WiringTest,
};
use crate::plan::schema::validation::{validate, validate_structural_preflight};

#[test]
fn test_validate_valid_metadata() {
    let metadata = create_valid_metadata();
    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_validate_unsupported_version() {
    // Use Knowledge stages to avoid goal-backward check errors
    let mut stage = make_stage("stage-1", "Stage One");
    stage.stage_type = StageType::Knowledge;

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 2, // Invalid version
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

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
            sandbox: SandboxConfig::default(),
            change_impact: None,
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
    let stage = make_stage("", "Test");

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
        .any(|e| e.message.contains("ID cannot be empty")));
}

#[test]
fn test_validate_empty_stage_name() {
    let stage = make_stage("stage-1", "");

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
        .any(|e| e.message.contains("name cannot be empty")));
}

#[test]
fn test_validate_unknown_dependency() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.dependencies = vec!["nonexistent".to_string()];

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
        .any(|e| e.message.contains("Unknown dependency")));
    assert!(errors.iter().any(|e| e.message.contains("nonexistent")));
}

#[test]
fn test_validate_self_dependency() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.dependencies = vec!["stage-1".to_string()];

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
        .any(|e| e.message.contains("cannot depend on itself")));
}

#[test]
fn test_validate_multiple_errors() {
    let stage1 = make_stage("", "");
    let mut stage2 = make_stage("stage-2", "Stage Two");
    stage2.dependencies = vec!["stage-2".to_string(), "nonexistent".to_string()];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 2,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage1, stage2],
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
    // New fields should also have defaults
    assert_eq!(stage.truth_checks.len(), 0);
    assert_eq!(stage.wiring_tests.len(), 0);
    assert!(stage.dead_code_check.is_none());
}

#[test]
fn test_complex_dependency_chain() {
    let mut stage1 = make_stage("stage-1", "Stage 1");
    stage1.truths = vec!["test -f README.md".to_string()];

    let mut stage2 = make_stage("stage-2", "Stage 2");
    stage2.dependencies = vec!["stage-1".to_string()];
    stage2.truths = vec!["test -f README.md".to_string()];

    let mut stage3 = make_stage("stage-3", "Stage 3");
    stage3.dependencies = vec!["stage-1".to_string(), "stage-2".to_string()];
    stage3.truths = vec!["test -f README.md".to_string()];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage1, stage2, stage3],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_validate_duplicate_stage_ids() {
    let stage1 = make_stage("stage-1", "Stage One");
    let stage2 = make_stage("stage-1", "Stage One Duplicate"); // Duplicate ID

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage1, stage2],
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
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = "../etc".to_string(); // Path traversal

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
    assert!(errors.iter().any(|e| e.message.contains("path traversal")));
}

#[test]
fn test_validate_working_dir_absolute_path() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = "/etc/passwd".to_string(); // Absolute path

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
        .any(|e| e.message.contains("must be relative path")));
}

#[test]
fn test_validate_working_dir_valid_subdirectory() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = "loom".to_string(); // Valid subdirectory
    stage.truths = vec!["cargo build".to_string()]; // Standard stages require goal-backward checks

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    assert!(validate(&metadata).is_ok());
}

// ============================================================================
// IntegrationVerify goal-backward requirement tests
// ============================================================================

#[test]
fn test_integration_verify_requires_goal_backward() {
    let mut stage = make_stage("integration-verify", "Integration Verify");
    stage.stage_type = StageType::IntegrationVerify;
    // No truths, artifacts, wiring, or wiring_tests - should fail

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
        .any(|e| e.message.contains("IntegrationVerify stages must define")));
}

#[test]
fn test_integration_verify_with_truths_passes() {
    let mut stage = make_stage("integration-verify", "Integration Verify");
    stage.stage_type = StageType::IntegrationVerify;
    stage.truths = vec!["cargo test".to_string()];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_integration_verify_with_wiring_tests_passes() {
    let mut stage = make_stage("integration-verify", "Integration Verify");
    stage.stage_type = StageType::IntegrationVerify;
    stage.wiring_tests = vec![WiringTest {
        name: "API endpoint test".to_string(),
        command: "curl -f localhost:8080/health".to_string(),
        success_criteria: Default::default(),
        description: Some("Verify API is reachable".to_string()),
    }];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_integration_verify_with_wiring_passes() {
    let mut stage = make_stage("integration-verify", "Integration Verify");
    stage.stage_type = StageType::IntegrationVerify;
    stage.wiring = vec![WiringCheck {
        source: "src/main.rs".to_string(),
        pattern: "feature_function".to_string(),
        description: "Feature is wired".to_string(),
    }];

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_knowledge_stage_exempt_from_goal_backward() {
    let mut stage = make_stage("knowledge-bootstrap", "Knowledge Bootstrap");
    stage.stage_type = StageType::Knowledge;
    // No truths/artifacts/wiring - should pass because Knowledge is exempt

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_code_review_stage_exempt_from_goal_backward() {
    let mut stage = make_stage("code-review", "Code Review");
    stage.stage_type = StageType::CodeReview;
    // No truths/artifacts/wiring - should pass because CodeReview is exempt

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    assert!(validate(&metadata).is_ok());
}

// ============================================================================
// Pre-flight validation tests
// ============================================================================

#[test]
fn test_preflight_warns_on_working_dir_redundancy() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = "loom".to_string();
    stage.acceptance = vec![
        "loom/src/main.rs".to_string(), // Redundant prefix
        "cargo test".to_string(),       // No redundancy
    ];
    stage.truths = vec!["test -f README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage]);
    assert!(!warnings.is_empty());
    assert!(warnings
        .iter()
        .any(|w| w.contains("redundant working_dir prefix")));
}

#[test]
fn test_preflight_no_warning_when_working_dir_is_dot() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = ".".to_string();
    stage.acceptance = vec!["loom/src/main.rs".to_string()];
    stage.truths = vec!["test -f README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage]);
    // No warnings when working_dir is "."
    assert!(warnings
        .iter()
        .all(|w| !w.contains("redundant working_dir prefix")));
}

#[test]
fn test_preflight_warns_on_short_wiring_pattern() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.wiring = vec![WiringCheck {
        source: "src/main.rs".to_string(),
        pattern: "fn".to_string(), // Too short
        description: "Test wiring".to_string(),
    }];
    stage.truths = vec!["test -f README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage]);
    assert!(!warnings.is_empty());
    assert!(warnings
        .iter()
        .any(|w| w.contains("significant characters")));
}

#[test]
fn test_preflight_warns_on_wildcard_pattern() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.wiring = vec![WiringCheck {
        source: "src/main.rs".to_string(),
        pattern: ".*".to_string(), // Matches everything
        description: "Test wiring".to_string(),
    }];
    stage.truths = vec!["test -f README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage]);
    assert!(!warnings.is_empty());
    assert!(warnings.iter().any(|w| w.contains("matches everything")));
}

#[test]
fn test_preflight_warns_on_single_char_pattern() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.wiring = vec![WiringCheck {
        source: "src/main.rs".to_string(),
        pattern: "x".to_string(), // Single character
        description: "Test wiring".to_string(),
    }];
    stage.truths = vec!["test -f README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage]);
    assert!(!warnings.is_empty());
    assert!(warnings
        .iter()
        .any(|w| w.contains("Single character patterns")));
}

#[test]
fn test_preflight_warns_on_generic_keyword() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.wiring = vec![WiringCheck {
        source: "src/main.rs".to_string(),
        pattern: "import".to_string(), // Common keyword (6 chars to pass length check)
        description: "Test wiring".to_string(),
    }];
    stage.truths = vec!["test -f README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage]);
    assert!(!warnings.is_empty());
    assert!(warnings.iter().any(|w| w.contains("common keyword")));
}

#[test]
fn test_preflight_no_warning_for_specific_pattern() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.wiring = vec![WiringCheck {
        source: "src/main.rs".to_string(),
        pattern: "validate_structural_preflight".to_string(), // Specific enough
        description: "Pre-flight validation exists".to_string(),
    }];
    stage.truths = vec!["test -f README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage]);
    // Should have no warnings about pattern quality for this specific pattern
    assert!(warnings
        .iter()
        .all(|w| !w.contains("significant characters") && !w.contains("common keyword")));
}
