//! Code review stage type tests
//!
//! Tests for:
//! - check_code_review_recommendations() function
//! - CodeReview exemption from goal-backward validation

use crate::plan::schema::types::{
    LoomConfig, LoomMetadata, SandboxConfig, StageDefinition, StageSandboxConfig, StageType,
};
use crate::plan::schema::validation::{check_code_review_recommendations, validate};

// ============================================================================
// check_code_review_recommendations() tests
// ============================================================================

#[test]
fn test_code_review_recommendations_no_dependencies_warning() {
    // CodeReview stage with no dependencies should trigger a warning
    let stages = vec![StageDefinition {
        id: "review-stage".to_string(),
        name: "Review".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::CodeReview,
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
    }];

    let warnings = check_code_review_recommendations(&stages);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
    assert!(warnings[0].contains("review-stage"));
}

#[test]
fn test_code_review_recommendations_with_dependencies_no_warning() {
    // CodeReview stage with dependencies should not trigger a warning
    let stages = vec![
        StageDefinition {
            id: "implement-feature".to_string(),
            name: "Implement".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::Standard,
            truths: vec!["test -f README.md".to_string()],
            artifacts: vec![],
            wiring: vec![],
            context_budget: None,
            sandbox: StageSandboxConfig::default(),
        },
        StageDefinition {
            id: "code-review".to_string(),
            name: "Code Review".to_string(),
            description: None,
            dependencies: vec!["implement-feature".to_string()],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::CodeReview,
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
            context_budget: None,
            sandbox: StageSandboxConfig::default(),
        },
    ];

    let warnings = check_code_review_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_code_review_recommendations_detected_by_id() {
    // Stage with "code-review" in ID should be detected as code review stage
    let stages = vec![StageDefinition {
        id: "my-code-review-stage".to_string(),
        name: "Review".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::Standard, // Not explicitly CodeReview type
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
    }];

    let warnings = check_code_review_recommendations(&stages);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
}

#[test]
fn test_code_review_recommendations_detected_by_name() {
    // Stage with "code review" in name should be detected as code review stage
    let stages = vec![StageDefinition {
        id: "review-stage".to_string(),
        name: "Final Code Review".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::Standard, // Not explicitly CodeReview type
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
    }];

    let warnings = check_code_review_recommendations(&stages);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
}

#[test]
fn test_code_review_recommendations_case_insensitive_id() {
    // Detection should be case-insensitive for ID
    let stages = vec![StageDefinition {
        id: "CODE-REVIEW-stage".to_string(),
        name: "Review".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::Standard,
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
    }];

    let warnings = check_code_review_recommendations(&stages);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
}

#[test]
fn test_code_review_recommendations_case_insensitive_name() {
    // Detection should be case-insensitive for name
    let stages = vec![StageDefinition {
        id: "review".to_string(),
        name: "CODE REVIEW Stage".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::Standard,
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
    }];

    let warnings = check_code_review_recommendations(&stages);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
}

#[test]
fn test_code_review_recommendations_empty_stages() {
    let stages: Vec<StageDefinition> = vec![];
    let warnings = check_code_review_recommendations(&stages);
    assert!(warnings.is_empty());
}

#[test]
fn test_code_review_recommendations_multiple_code_review_stages() {
    // Multiple CodeReview stages without dependencies should each get a warning
    let stages = vec![
        StageDefinition {
            id: "review-1".to_string(),
            name: "Review One".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::CodeReview,
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
            context_budget: None,
            sandbox: StageSandboxConfig::default(),
        },
        StageDefinition {
            id: "review-2".to_string(),
            name: "Review Two".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::CodeReview,
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
            context_budget: None,
            sandbox: StageSandboxConfig::default(),
        },
    ];

    let warnings = check_code_review_recommendations(&stages);
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().any(|w| w.contains("review-1")));
    assert!(warnings.iter().any(|w| w.contains("review-2")));
}

#[test]
fn test_code_review_recommendations_non_code_review_stage_no_warning() {
    // Non-code-review stages without dependencies should not trigger code review warnings
    let stages = vec![StageDefinition {
        id: "setup-stage".to_string(),
        name: "Setup".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::Standard,
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        context_budget: None,
        sandbox: StageSandboxConfig::default(),
    }];

    let warnings = check_code_review_recommendations(&stages);
    assert!(warnings.is_empty());
}

// ============================================================================
// CodeReview exemption from goal-backward validation tests
// ============================================================================

#[test]
fn test_code_review_stage_exempt_from_goal_backward_validation() {
    // CodeReview stage without goal-backward checks should pass validation
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            stages: vec![StageDefinition {
                id: "code-review".to_string(),
                name: "Code Review".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::CodeReview,
                truths: vec![],     // No truths
                artifacts: vec![],  // No artifacts
                wiring: vec![],     // No wiring
                context_budget: None,
                sandbox: StageSandboxConfig::default(),
            }],
        },
    };

    // Should pass validation (CodeReview is exempt from goal-backward requirements)
    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_standard_stage_requires_goal_backward_validation() {
    // Standard stage without goal-backward checks should fail validation
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            stages: vec![StageDefinition {
                id: "implement-feature".to_string(),
                name: "Implement Feature".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::Standard,
                truths: vec![],     // No truths
                artifacts: vec![],  // No artifacts
                wiring: vec![],     // No wiring
                context_budget: None,
                sandbox: StageSandboxConfig::default(),
            }],
        },
    };

    // Should fail validation (Standard stages require goal-backward checks)
    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("must define at least one truth")));
}

#[test]
fn test_knowledge_stage_exempt_from_goal_backward_validation() {
    // Knowledge stage without goal-backward checks should pass validation
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            stages: vec![StageDefinition {
                id: "knowledge-bootstrap".to_string(),
                name: "Knowledge Bootstrap".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::Knowledge,
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
                sandbox: StageSandboxConfig::default(),
            }],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_integration_verify_stage_exempt_from_goal_backward_validation() {
    // IntegrationVerify stage without goal-backward checks should pass validation
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            stages: vec![StageDefinition {
                id: "integration-verify".to_string(),
                name: "Integration Verification".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::IntegrationVerify,
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
                sandbox: StageSandboxConfig::default(),
            }],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_code_review_stage_can_still_have_goal_backward_checks() {
    // CodeReview stage with goal-backward checks should pass validation
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            stages: vec![StageDefinition {
                id: "code-review".to_string(),
                name: "Code Review".to_string(),
                description: None,
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                auto_merge: None,
                working_dir: ".".to_string(),
                stage_type: StageType::CodeReview,
                truths: vec!["test -f REVIEW_COMPLETE.md".to_string()],
                artifacts: vec![],
                wiring: vec![],
                context_budget: None,
                sandbox: StageSandboxConfig::default(),
            }],
        },
    };

    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_stage_type_serde_code_review() {
    // Test that CodeReview stage type serializes/deserializes correctly
    let yaml = r#"
id: review-stage
name: Review Stage
working_dir: "."
stage_type: code-review
"#;
    let stage: StageDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(stage.stage_type, StageType::CodeReview);
}

#[test]
fn test_mixed_stage_types_validation() {
    // Test validation with mixed stage types
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            stages: vec![
                // Knowledge stage (exempt from goal-backward)
                StageDefinition {
                    id: "knowledge-bootstrap".to_string(),
                    name: "Knowledge Bootstrap".to_string(),
                    description: None,
                    dependencies: vec![],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::Knowledge,
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                    sandbox: StageSandboxConfig::default(),
                },
                // Standard stage (requires goal-backward)
                StageDefinition {
                    id: "implement".to_string(),
                    name: "Implement".to_string(),
                    description: None,
                    dependencies: vec!["knowledge-bootstrap".to_string()],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::Standard,
                    truths: vec!["cargo build".to_string()],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                    sandbox: StageSandboxConfig::default(),
                },
                // CodeReview stage (exempt from goal-backward)
                StageDefinition {
                    id: "code-review".to_string(),
                    name: "Code Review".to_string(),
                    description: None,
                    dependencies: vec!["implement".to_string()],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::CodeReview,
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                    sandbox: StageSandboxConfig::default(),
                },
                // IntegrationVerify stage (exempt from goal-backward)
                StageDefinition {
                    id: "integration-verify".to_string(),
                    name: "Integration Verification".to_string(),
                    description: None,
                    dependencies: vec!["code-review".to_string()],
                    parallel_group: None,
                    acceptance: vec![],
                    setup: vec![],
                    files: vec![],
                    auto_merge: None,
                    working_dir: ".".to_string(),
                    stage_type: StageType::IntegrationVerify,
                    truths: vec![],
                    artifacts: vec![],
                    wiring: vec![],
                    context_budget: None,
                    sandbox: StageSandboxConfig::default(),
                },
            ],
        },
    };

    // Should pass - all stages correctly configured
    assert!(validate(&metadata).is_ok());
}
