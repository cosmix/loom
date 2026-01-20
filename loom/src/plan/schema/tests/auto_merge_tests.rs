//! Auto-merge configuration parsing tests

use crate::plan::schema::types::LoomMetadata;

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
