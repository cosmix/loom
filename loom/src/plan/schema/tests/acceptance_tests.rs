//! Acceptance criterion validation tests

use super::make_stage;
use crate::plan::schema::types::{AcceptanceCriterion, LoomConfig, LoomMetadata, SandboxConfig};
use crate::plan::schema::validation::{validate, validate_acceptance_criterion};

#[test]
fn test_validate_acceptance_criterion_valid() {
    assert!(
        validate_acceptance_criterion(&AcceptanceCriterion::Simple("cargo test".to_string()))
            .is_ok()
    );
    assert!(validate_acceptance_criterion(&AcceptanceCriterion::Simple(
        "cargo build --release".to_string()
    ))
    .is_ok());
    assert!(validate_acceptance_criterion(&AcceptanceCriterion::Simple(
        "npm run test && npm run lint".to_string()
    ))
    .is_ok());
    assert!(validate_acceptance_criterion(&AcceptanceCriterion::Simple(
        "cd loom && cargo test --lib".to_string()
    ))
    .is_ok());
}

#[test]
fn test_validate_acceptance_criterion_empty() {
    assert!(validate_acceptance_criterion(&AcceptanceCriterion::Simple("".to_string())).is_err());
    assert!(
        validate_acceptance_criterion(&AcceptanceCriterion::Simple("   ".to_string())).is_err()
    );
    assert!(
        validate_acceptance_criterion(&AcceptanceCriterion::Simple("\t\n".to_string())).is_err()
    );
}

#[test]
fn test_validate_acceptance_criterion_too_long() {
    let long_criterion = "a".repeat(1025);
    let result = validate_acceptance_criterion(&AcceptanceCriterion::Simple(long_criterion));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("too long"));
}

#[test]
fn test_validate_acceptance_criterion_control_chars() {
    // Null byte
    let result =
        validate_acceptance_criterion(&AcceptanceCriterion::Simple("cargo\x00test".to_string()));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("control character"));

    // Bell character
    let result =
        validate_acceptance_criterion(&AcceptanceCriterion::Simple("cargo\x07test".to_string()));
    assert!(result.is_err());
}

#[test]
fn test_validate_acceptance_criterion_allowed_whitespace() {
    // Tab, newline, carriage return should be allowed
    assert!(validate_acceptance_criterion(&AcceptanceCriterion::Simple(
        "cargo test\t--lib".to_string()
    ))
    .is_ok());
    assert!(validate_acceptance_criterion(&AcceptanceCriterion::Simple(
        "cargo test\n".to_string()
    ))
    .is_ok());
}

#[test]
fn test_validate_metadata_with_empty_acceptance() {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.acceptance = vec![AcceptanceCriterion::Simple("".to_string())];

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
        AcceptanceCriterion::Simple("cargo test".to_string()),
        AcceptanceCriterion::Simple("cargo clippy -- -D warnings".to_string()),
    ];

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
    stage.acceptance = vec![
        AcceptanceCriterion::Simple("".to_string()),
        AcceptanceCriterion::Simple("   ".to_string()),
        AcceptanceCriterion::Simple("cargo test".to_string()),
    ];

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

#[test]
fn test_acceptance_criterion_yaml_simple_string() {
    let yaml = r#""echo hello""#;
    let result: Result<AcceptanceCriterion, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "Simple string should deserialize: {:?}",
        result.err()
    );
    assert_eq!(
        result.unwrap(),
        AcceptanceCriterion::Simple("echo hello".to_string())
    );
}

#[test]
fn test_acceptance_criterion_yaml_extended_object() {
    let yaml = r#"
command: "echo test output"
stdout_contains: ["test output"]
exit_code: 0
"#;
    let result: Result<AcceptanceCriterion, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "Extended object should deserialize: {:?}",
        result.err()
    );
    let criterion = result.unwrap();
    assert!(criterion.is_extended());
    assert_eq!(criterion.command(), "echo test output");
}

#[test]
fn test_acceptance_criterion_yaml_mixed_list() {
    let yaml = r#"
- "echo hello"
- command: "echo test output"
  stdout_contains: ["test output"]
  exit_code: 0
"#;
    let result: Result<Vec<AcceptanceCriterion>, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "Mixed list should deserialize: {:?}",
        result.err()
    );
    let criteria = result.unwrap();
    assert_eq!(criteria.len(), 2);
    assert_eq!(
        criteria[0],
        AcceptanceCriterion::Simple("echo hello".to_string())
    );
    assert!(criteria[1].is_extended());
}

#[test]
fn test_acceptance_criterion_yaml_full_plan() {
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: test-stage
      name: "Test Stage"
      working_dir: "."
      stage_type: standard
      acceptance:
        - "echo hello"
        - command: "echo test output"
          stdout_contains: ["test output"]
          exit_code: 0
      artifacts:
        - "README.md"
"#;
    let result: Result<crate::plan::schema::types::LoomMetadata, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "Full plan with mixed acceptance should parse: {:?}",
        result.err()
    );
    let metadata = result.unwrap();
    let stage = &metadata.loom.stages[0];
    assert_eq!(stage.acceptance.len(), 2);
    assert_eq!(
        stage.acceptance[0],
        AcceptanceCriterion::Simple("echo hello".to_string())
    );
    assert!(stage.acceptance[1].is_extended());
}

#[test]
fn test_old_plan_with_truths_field_parses() {
    // Old plans had a truths: Vec<String> field that no longer exists.
    // Since StageDefinition does NOT use deny_unknown_fields, serde should
    // silently ignore the unknown truths field.
    let yaml = r#"
loom:
  version: 1
  stages:
    - id: old-stage
      name: "Old Format"
      working_dir: "."
      stage_type: standard
      acceptance:
        - "echo ok"
      truths:
        - "echo hello"
      truth_checks:
        - command: "echo world"
          stdout_contains: ["world"]
      artifacts:
        - "README.md"
"#;
    let result: Result<crate::plan::schema::types::LoomMetadata, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "Old plan with truths/truth_checks should parse: {:?}",
        result.err()
    );
    let metadata = result.unwrap();
    let stage = &metadata.loom.stages[0];
    // truths field should be silently dropped since it no longer exists on StageDefinition
    assert_eq!(stage.acceptance.len(), 1);
    assert_eq!(
        stage.acceptance[0],
        AcceptanceCriterion::Simple("echo ok".to_string())
    );
}
