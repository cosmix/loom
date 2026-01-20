//! Tests for stage serialization and frontmatter parsing

use crate::models::stage::StageStatus;
use crate::parser::frontmatter::extract_yaml_frontmatter;
use crate::verify::transitions::{parse_stage_from_markdown, serialize_stage_to_markdown};

use super::create_test_stage;

#[test]
fn test_serialize_and_parse_roundtrip() {
    let mut stage = create_test_stage("stage-1", "Test Stage", StageStatus::WaitingForDeps);
    stage.add_dependency("stage-0".to_string());
    stage.add_acceptance_criterion("Criterion 1".to_string());
    stage.add_acceptance_criterion("Criterion 2".to_string());
    stage.add_file_pattern("src/**/*.rs".to_string());

    let markdown = serialize_stage_to_markdown(&stage).expect("Should serialize");

    let parsed = parse_stage_from_markdown(&markdown).expect("Should parse");

    assert_eq!(parsed.id, stage.id);
    assert_eq!(parsed.name, stage.name);
    assert_eq!(parsed.status, stage.status);
    assert_eq!(parsed.dependencies, stage.dependencies);
    assert_eq!(parsed.acceptance, stage.acceptance);
    assert_eq!(parsed.files, stage.files);
}

#[test]
fn test_extract_yaml_frontmatter() {
    let content = r#"---
id: stage-1
name: Test
status: Pending
---

# Body content"#;

    let yaml = extract_yaml_frontmatter(content).expect("Should extract frontmatter");
    assert!(yaml.is_mapping());

    let map = yaml.as_mapping().unwrap();
    assert_eq!(
        map.get(serde_yaml::Value::String("id".to_string()))
            .unwrap()
            .as_str()
            .unwrap(),
        "stage-1"
    );
}

#[test]
fn test_extract_yaml_frontmatter_missing_delimiter() {
    let content = "id: stage-1\nname: Test";

    let result = extract_yaml_frontmatter(content);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No frontmatter"));
}

#[test]
fn test_extract_yaml_frontmatter_unclosed() {
    let content = "---\nid: stage-1\nname: Test";

    let result = extract_yaml_frontmatter(content);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("not properly closed"));
}
