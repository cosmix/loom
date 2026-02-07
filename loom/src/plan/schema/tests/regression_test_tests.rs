//! Tests for regression test validation

use super::{create_valid_metadata, make_stage};
use crate::plan::schema::types::{RegressionTest, StageType};
use crate::plan::schema::validation::validate;

#[test]
fn test_bug_fix_without_regression_test_fails() {
    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("bug-fix-stage", "Bug Fix");
    stage.bug_fix = Some(true);
    stage.truths = vec!["cargo test".to_string()]; // Standard stage needs verification
    metadata.loom.stages.push(stage);

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("bug_fix stages must define a regression_test")));
}

#[test]
fn test_bug_fix_with_regression_test_passes() {
    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("bug-fix-stage", "Bug Fix");
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: "tests/regression_test.rs".to_string(),
        must_contain: vec!["test_bug_fixed".to_string()],
    });
    stage.truths = vec!["cargo test".to_string()]; // Standard stage needs verification
    metadata.loom.stages.push(stage);

    let result = validate(&metadata);
    assert!(result.is_ok());
}

#[test]
fn test_regression_test_without_bug_fix_fails() {
    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("normal-stage", "Normal Stage");
    stage.regression_test = Some(RegressionTest {
        file: "tests/some_test.rs".to_string(),
        must_contain: vec!["test_something".to_string()],
    });
    stage.truths = vec!["cargo test".to_string()]; // Standard stage needs verification
    metadata.loom.stages.push(stage);

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("regression_test defined but bug_fix is not true")));
}

#[test]
fn test_regression_test_empty_file_fails() {
    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("bug-fix-stage", "Bug Fix");
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: "".to_string(),
        must_contain: vec!["test_bug_fixed".to_string()],
    });
    stage.truths = vec!["cargo test".to_string()];
    metadata.loom.stages.push(stage);

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("regression_test.file cannot be empty")));
}

#[test]
fn test_regression_test_path_traversal_fails() {
    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("bug-fix-stage", "Bug Fix");
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: "../tests/regression_test.rs".to_string(),
        must_contain: vec!["test_bug_fixed".to_string()],
    });
    stage.truths = vec!["cargo test".to_string()];
    metadata.loom.stages.push(stage);

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("regression_test.file cannot contain path traversal")));
}

#[test]
fn test_regression_test_absolute_path_fails() {
    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("bug-fix-stage", "Bug Fix");
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: "/tmp/tests/regression_test.rs".to_string(),
        must_contain: vec!["test_bug_fixed".to_string()],
    });
    stage.truths = vec!["cargo test".to_string()];
    metadata.loom.stages.push(stage);

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("regression_test.file must be relative path")));
}

#[test]
fn test_regression_test_empty_must_contain_fails() {
    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("bug-fix-stage", "Bug Fix");
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: "tests/regression_test.rs".to_string(),
        must_contain: vec![],
    });
    stage.truths = vec!["cargo test".to_string()];
    metadata.loom.stages.push(stage);

    let result = validate(&metadata);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("regression_test.must_contain must have at least one pattern")));
}

#[test]
fn test_regression_test_serde_defaults() {
    use serde_yaml;

    let yaml = r#"
file: "tests/regression_test.rs"
"#;

    let rt: RegressionTest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rt.file, "tests/regression_test.rs");
    assert!(rt.must_contain.is_empty());
}

#[test]
fn test_full_plan_with_regression_test_yaml() {
    use crate::plan::schema::types::LoomMetadata;
    use serde_yaml;

    let yaml = r#"
loom:
  version: 1
  sandbox:
    enabled: true
  stages:
    - id: bug-fix-stage
      name: Bug Fix Stage
      working_dir: "."
      stage_type: standard
      bug_fix: true
      regression_test:
        file: tests/regression_test.rs
        must_contain:
          - test_bug_fixed
          - assert_eq
      truths:
        - cargo test
"#;

    // Deserialize
    let metadata: LoomMetadata = serde_yaml::from_str(yaml).unwrap();

    // Verify structure
    assert_eq!(metadata.loom.stages.len(), 1);
    let stage = &metadata.loom.stages[0];
    assert_eq!(stage.id, "bug-fix-stage");
    assert_eq!(stage.bug_fix, Some(true));

    let rt = stage.regression_test.as_ref().unwrap();
    assert_eq!(rt.file, "tests/regression_test.rs");
    assert_eq!(rt.must_contain.len(), 2);
    assert!(rt.must_contain.contains(&"test_bug_fixed".to_string()));
    assert!(rt.must_contain.contains(&"assert_eq".to_string()));

    // Validate
    let result = validate(&metadata);
    assert!(result.is_ok());
}

#[test]
fn test_regression_test_serialization_round_trip() {
    use serde_yaml;

    let mut metadata = create_valid_metadata();
    let mut stage = make_stage("bug-fix", "Bug Fix Stage");
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: "tests/bug_regression.rs".to_string(),
        must_contain: vec![
            "test_original_bug".to_string(),
            "test_edge_case".to_string(),
        ],
    });
    stage.truths = vec!["cargo test bug_regression".to_string()];
    metadata.loom.stages.push(stage);

    // Serialize to YAML
    let yaml = serde_yaml::to_string(&metadata).unwrap();

    // Deserialize back
    let deserialized: super::LoomMetadata = serde_yaml::from_str(&yaml).unwrap();

    // Verify the regression_test survived the round trip
    let bug_stage = deserialized
        .loom
        .stages
        .iter()
        .find(|s| s.id == "bug-fix")
        .unwrap();
    assert_eq!(bug_stage.bug_fix, Some(true));

    let rt = bug_stage.regression_test.as_ref().unwrap();
    assert_eq!(rt.file, "tests/bug_regression.rs");
    assert_eq!(rt.must_contain.len(), 2);
}

#[test]
fn test_preflight_warns_regression_test_working_dir_prefix() {
    use crate::plan::schema::validation::validate_structural_preflight;

    let mut stage = make_stage("bug-fix-stage", "Bug Fix");
    stage.working_dir = "loom".to_string();
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: "loom/tests/regression_test.rs".to_string(),
        must_contain: vec!["test_bug_fixed".to_string()],
    });
    // Use Knowledge stage to skip goal-backward verification requirement
    stage.stage_type = StageType::Knowledge;

    let dir = tempfile::TempDir::new().unwrap();
    let warnings = validate_structural_preflight(&[stage], Some(dir.path()));
    assert!(warnings
        .iter()
        .any(|w| w.contains("regression_test.file") && w.contains("redundant working_dir prefix")));
}
