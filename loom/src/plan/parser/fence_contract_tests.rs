//! Contract: an inline triple backtick inside a YAML string does not close
//! the metadata fence; only a fence on its own line does.

use crate::plan::parser::{extract_yaml_metadata, parse_plan_content};
use std::path::Path;

const DESCRIPTION: &str = "wrap code in ```rust``` fences";

fn plan_text() -> String {
    format!(
        "# Fence Plan\n\n<!-- loom METADATA -->\n```yaml\nloom:\n  version: 1\n  stages:\n    - id: s1\n      name: S\n      working_dir: \".\"\n      description: \"{DESCRIPTION}\"\n      artifacts:\n        - README.md\n      wiring:\n        - source: README.md\n          pattern: readme\n          description: the readme is present\n```\n<!-- END loom METADATA -->\n"
    )
}

#[test]
fn inline_backticks_in_a_yaml_string_do_not_end_the_block() {
    let text = plan_text();

    let parsed = parse_plan_content(&text, Path::new("PLAN-fence.md"))
        .unwrap_or_else(|e| panic!("the plan must parse: {e:#}"));
    assert_eq!(parsed.stages[0].description.as_deref(), Some(DESCRIPTION));

    let body = extract_yaml_metadata(&text).unwrap_or_else(|e| panic!("{e:#}"));
    let line = format!("description: \"{DESCRIPTION}\"");
    assert!(body.contains(&line), "the body was cut short: {body:?}");
}
