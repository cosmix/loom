//! Code review stage type tests
//!
//! Tests for:
//! - check_code_review_recommendations() function
//! - CodeReview exemption from goal-backward validation

use super::make_stage;
use crate::plan::schema::types::{
    LoomConfig, LoomMetadata, SandboxConfig, StageDefinition, StageType,
};
use crate::plan::schema::validation::{check_code_review_recommendations, validate};

// ============================================================================
// check_code_review_recommendations() tests
// ============================================================================

#[test]
fn test_code_review_recommendations_no_dependencies_warning() {
    // CodeReview stage with no dependencies should trigger a warning
    let mut stage = make_stage("review-stage", "Review");
    stage.stage_type = StageType::CodeReview;

    let warnings = check_code_review_recommendations(&[stage]);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
    assert!(warnings[0].contains("review-stage"));
}

#[test]
fn test_code_review_recommendations_with_dependencies_no_warning() {
    // CodeReview stage with dependencies should not trigger a warning
    let mut impl_stage = make_stage("implement-feature", "Implement");
    impl_stage.truths = vec!["test -f README.md".to_string()];

    let mut review_stage = make_stage("code-review", "Code Review");
    review_stage.dependencies = vec!["implement-feature".to_string()];
    review_stage.stage_type = StageType::CodeReview;

    let warnings = check_code_review_recommendations(&[impl_stage, review_stage]);
    assert!(warnings.is_empty());
}

#[test]
fn test_code_review_recommendations_detected_by_id() {
    // Stage with "code-review" in ID should be detected as code review stage
    let stage = make_stage("my-code-review-stage", "Review");

    let warnings = check_code_review_recommendations(&[stage]);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
}

#[test]
fn test_code_review_recommendations_detected_by_name() {
    // Stage with "code review" in name should be detected as code review stage
    let stage = make_stage("review-stage", "Final Code Review");

    let warnings = check_code_review_recommendations(&[stage]);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
}

#[test]
fn test_code_review_recommendations_case_insensitive_id() {
    // Detection should be case-insensitive for ID
    let stage = make_stage("CODE-REVIEW-stage", "Review");

    let warnings = check_code_review_recommendations(&[stage]);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no dependencies"));
}

#[test]
fn test_code_review_recommendations_case_insensitive_name() {
    // Detection should be case-insensitive for name
    let stage = make_stage("review", "CODE REVIEW Stage");

    let warnings = check_code_review_recommendations(&[stage]);
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
    let mut review1 = make_stage("review-1", "Review One");
    review1.stage_type = StageType::CodeReview;

    let mut review2 = make_stage("review-2", "Review Two");
    review2.stage_type = StageType::CodeReview;

    let warnings = check_code_review_recommendations(&[review1, review2]);
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().any(|w| w.contains("review-1")));
    assert!(warnings.iter().any(|w| w.contains("review-2")));
}

#[test]
fn test_code_review_recommendations_non_code_review_stage_no_warning() {
    // Non-code-review stages without dependencies should not trigger code review warnings
    let stage = make_stage("setup-stage", "Setup");

    let warnings = check_code_review_recommendations(&[stage]);
    assert!(warnings.is_empty());
}

// ============================================================================
// CodeReview exemption from goal-backward validation tests
// ============================================================================

#[test]
fn test_code_review_stage_exempt_from_goal_backward_validation() {
    // CodeReview stage without goal-backward checks should pass validation
    let mut stage = make_stage("code-review", "Code Review");
    stage.stage_type = StageType::CodeReview;

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    // Should pass validation (CodeReview is exempt from goal-backward requirements)
    assert!(validate(&metadata).is_ok());
}

#[test]
fn test_standard_stage_requires_goal_backward_validation() {
    // Standard stage without goal-backward checks should fail validation
    let stage = make_stage("implement-feature", "Implement Feature");

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![stage],
        },
    };

    // Should fail validation (Standard stages require goal-backward checks)
    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("must define at least one truth")));
}

#[test]
fn test_knowledge_stage_exempt_from_goal_backward_validation() {
    // Knowledge stage without goal-backward checks should pass validation
    let mut stage = make_stage("knowledge-bootstrap", "Knowledge Bootstrap");
    stage.stage_type = StageType::Knowledge;

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
fn test_integration_verify_stage_exempt_from_goal_backward_validation() {
    // IntegrationVerify stage without goal-backward checks should pass validation
    let mut stage = make_stage("integration-verify", "Integration Verification");
    stage.stage_type = StageType::IntegrationVerify;

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
fn test_code_review_stage_can_still_have_goal_backward_checks() {
    // CodeReview stage with goal-backward checks should pass validation
    let mut stage = make_stage("code-review", "Code Review");
    stage.stage_type = StageType::CodeReview;
    stage.truths = vec!["test -f REVIEW_COMPLETE.md".to_string()];

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
    let mut knowledge = make_stage("knowledge-bootstrap", "Knowledge Bootstrap");
    knowledge.stage_type = StageType::Knowledge;

    let mut implement = make_stage("implement", "Implement");
    implement.dependencies = vec!["knowledge-bootstrap".to_string()];
    implement.truths = vec!["cargo build".to_string()];

    let mut review = make_stage("code-review", "Code Review");
    review.dependencies = vec!["implement".to_string()];
    review.stage_type = StageType::CodeReview;

    let mut verify = make_stage("integration-verify", "Integration Verification");
    verify.dependencies = vec!["code-review".to_string()];
    verify.stage_type = StageType::IntegrationVerify;

    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            stages: vec![knowledge, implement, review, verify],
        },
    };

    // Should pass - all stages correctly configured
    assert!(validate(&metadata).is_ok());
}
