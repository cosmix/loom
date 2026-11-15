//! Contracts for the shared frontmatter boundary (plan Decision A8) and the
//! shared plan-metadata parse (plan Decision A6).

use crate::fs::knowledge::frontmatter::{split_frontmatter, Frontmatter};
use crate::parser::frontmatter::split_frontmatter_raw;
use crate::plan::parser::{parse_and_validate, parse_metadata};

#[test]
fn four_dash_line_does_not_close() {
    let content = "---\na: 1\n----\nb: 2\n---\nbody\n";
    let (yaml, body) = split_frontmatter_raw(content).expect("frontmatter closes at `---`");
    assert_eq!(yaml, "a: 1\n----\nb: 2\n");
    assert_eq!(body, "body\n");
}

#[test]
fn knowledge_closer_with_trailing_space() {
    let text = "---\nkey: [unclosed\n---  \nbody\n";
    let (metadata, body) = split_frontmatter(text);
    assert_eq!(metadata, Frontmatter::default());
    assert_eq!(body, "body\n");
}

const RETIRED_TRUTHS_PLAN: &str = r#"
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage One"
      working_dir: "."
      truths: ["x"]
"#;

#[test]
fn metadata_parse_keeps_retired_field_guidance() {
    let error = parse_metadata(RETIRED_TRUTHS_PLAN).expect_err("`truths` is a retired field");
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("unknown field `truths`"),
        "error names the field: {rendered}"
    );
    assert!(
        rendered.contains("Move each entry into `acceptance`"),
        "error carries the retired-field guidance: {rendered}"
    );
}

const DUPLICATE_ID_PLAN: &str = r#"
loom:
  version: 1
  stages:
    - id: a
      name: "First"
      working_dir: "."
    - id: a
      name: "Second"
      working_dir: "."
"#;

#[test]
fn metadata_parse_does_not_validate() {
    assert!(
        parse_and_validate(DUPLICATE_ID_PLAN).is_err(),
        "the scenario plan must fail validation"
    );
    let metadata = parse_metadata(DUPLICATE_ID_PLAN).expect("schema parse skips validation");
    let names: Vec<&str> = metadata
        .loom
        .stages
        .iter()
        .map(|stage| stage.name.as_str())
        .collect();
    assert_eq!(names, ["First", "Second"]);
    assert!(metadata.loom.stages.iter().all(|stage| stage.id == "a"));
}
