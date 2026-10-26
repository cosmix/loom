//! Tests for `loom plan check`.

use std::fs;

use super::{human, report};
use crate::planning::project::Project;

#[test]
fn report_serializes_the_contract_keys() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("doc")).unwrap();
    let project = Project::load(temp.path()).unwrap();
    let value = serde_json::to_value(report(&project)).unwrap();
    let keys: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["checklist", "problems", "root", "schema"]);
    let checklist = value["checklist"].as_object().unwrap();
    let keys: Vec<_> = checklist.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        ["current", "items", "primary", "secondary", "state", "steps"]
    );
    assert_eq!(value["schema"], 1);
    assert_eq!(value["checklist"]["steps"].as_array().unwrap().len(), 7);
}

#[test]
fn human_output_neutralizes_escape_in_section_title() {
    let temp = tempfile::tempdir().unwrap();
    let product = temp.path().join("doc/product");
    fs::create_dir_all(&product).unwrap();
    fs::write(product.join("index.md"), "# Vision\n\n## Specification sections\n\n| Number | Name |\n| --- | --- |\n| 1 | Hello\x1bworld |\n").unwrap();
    let project = Project::load(temp.path()).unwrap();
    let output = human(&project);
    assert!(output.contains("Hello"), "{output}");
    assert!(!output.contains('\x1b'), "{output:?}");
}
