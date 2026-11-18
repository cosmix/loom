//! E2E tests for auto-merge workflow

use chrono::Utc;
use loom::models::stage::{Stage, StageStatus};
use loom::orchestrator::auto_merge::is_auto_merge_enabled;
use loom::plan::schema::{LoomMetadata, StageDefinition};

// Helper to create a test stage
fn create_test_stage(id: &str, auto_merge: Option<bool>) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Test Stage {id}"),
        status: StageStatus::Completed,
        worktree: Some(id.to_string()),
        completed_at: Some(Utc::now()),
        auto_merge,
        working_dir: None,
        ..Stage::default()
    }
}

#[test]
fn test_parse_auto_merge_from_yaml() {
    let yaml = r#"
loom:
  version: 1
  auto_merge: true
  stages:
    - id: stage-1
      name: "First Stage"
      auto_merge: false
      working_dir: "."
    - id: stage-2
      name: "Second Stage"
      working_dir: "."
"#;

    let metadata: LoomMetadata = loom::yaml::from_str(yaml).unwrap();

    // Plan-level auto_merge
    assert_eq!(metadata.loom.auto_merge, Some(true));

    // Stage-level overrides
    assert_eq!(metadata.loom.stages[0].auto_merge, Some(false));
    assert_eq!(metadata.loom.stages[1].auto_merge, None);
}

#[test]
fn test_auto_merge_all_combinations() {
    // Precedence is stage > plan > orchestrator, over all 18 combinations of the
    // boolean and two optional flags.
    // Format: (orchestrator, plan, stage, expected_result)
    let test_cases = [
        (false, None, None, false),
        (false, None, Some(false), false),
        (false, None, Some(true), true),
        (false, Some(false), None, false),
        (false, Some(false), Some(false), false),
        (false, Some(false), Some(true), true),
        (false, Some(true), None, true),
        (false, Some(true), Some(false), false),
        (false, Some(true), Some(true), true),
        (true, None, None, true),
        (true, None, Some(false), false),
        (true, None, Some(true), true),
        (true, Some(false), None, false),
        (true, Some(false), Some(false), false),
        (true, Some(false), Some(true), true),
        (true, Some(true), None, true),
        (true, Some(true), Some(false), false),
        (true, Some(true), Some(true), true),
    ];

    for (idx, (orchestrator, plan, stage_merge, expected)) in test_cases.iter().enumerate() {
        let stage = create_test_stage(&format!("test-{idx}"), *stage_merge);
        let result = is_auto_merge_enabled(&stage, *orchestrator, *plan);
        assert_eq!(
            result, *expected,
            "Test case {idx} failed: orchestrator={orchestrator}, plan={plan:?}, stage={stage_merge:?}, expected={expected}, got={result}",
        );
    }
}

#[test]
fn test_stage_definition_with_auto_merge() {
    let yaml = r#"
id: test-stage
name: Test Stage
auto_merge: true
working_dir: "."
"#;
    let stage: StageDefinition = loom::yaml::from_str(yaml).unwrap();
    assert_eq!(stage.auto_merge, Some(true));
}

#[test]
fn test_stage_definition_without_auto_merge() {
    let yaml = r#"
id: test-stage
name: Test Stage
working_dir: "."
"#;
    let stage: StageDefinition = loom::yaml::from_str(yaml).unwrap();
    assert_eq!(stage.auto_merge, None);
}

#[test]
fn test_loom_config_with_auto_merge() {
    let yaml = r#"
loom:
  version: 1
  auto_merge: false
  stages:
    - id: stage-1
      name: "Stage 1"
      working_dir: "."
"#;
    let metadata: LoomMetadata = loom::yaml::from_str(yaml).unwrap();
    assert_eq!(metadata.loom.auto_merge, Some(false));
}

#[test]
fn test_mixed_auto_merge_configuration() {
    let yaml = r#"
loom:
  version: 1
  auto_merge: true
  stages:
    - id: stage-1
      name: "Force Disabled"
      auto_merge: false
      working_dir: "."
    - id: stage-2
      name: "Force Enabled"
      auto_merge: true
      working_dir: "."
    - id: stage-3
      name: "Use Default"
      working_dir: "."
"#;

    let metadata: LoomMetadata = loom::yaml::from_str(yaml).unwrap();

    // Plan level
    assert_eq!(metadata.loom.auto_merge, Some(true));

    // Stage 1 overrides to false
    let stage1 = create_test_stage("stage-1", metadata.loom.stages[0].auto_merge);
    assert!(!is_auto_merge_enabled(
        &stage1,
        false,
        metadata.loom.auto_merge
    ));

    // Stage 2 overrides to true
    let stage2 = create_test_stage("stage-2", metadata.loom.stages[1].auto_merge);
    assert!(is_auto_merge_enabled(
        &stage2,
        false,
        metadata.loom.auto_merge
    ));

    // Stage 3 uses plan default (true)
    let stage3 = create_test_stage("stage-3", metadata.loom.stages[2].auto_merge);
    assert!(is_auto_merge_enabled(
        &stage3,
        false,
        metadata.loom.auto_merge
    ));
}
