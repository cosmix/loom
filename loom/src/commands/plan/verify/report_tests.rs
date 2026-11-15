//! The metadata parse failure `loom plan verify` shows.

use std::path::Path;

use super::report::verify_text;

fn plan_with_stage_field(field: &str) -> String {
    format!(
        "# Retired\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  stages:\n    \
         - id: a\n      name: \"A\"\n      working_dir: \".\"\n      {field}: [x]\n```\n\n\
         <!-- END loom METADATA -->\n"
    )
}

/// A retired stage field shows the migration guidance the plan panel and
/// `parse_and_validate` show, not serde's bare `unknown field` error.
#[test]
fn a_retired_field_shows_the_migration_guidance() {
    let report = verify_text(
        None,
        Path::new("PLAN-retired.md"),
        &plan_with_stage_field("truths"),
    );

    assert!(report.stopped);
    let message = &report.errors[0].message;
    assert!(message.starts_with("YAML parse error: "), "{message}");
    assert!(message.contains("unknown field `truths`"), "{message}");
    assert!(
        message.contains("Move each entry into `acceptance`"),
        "{message}"
    );
}

/// The context line `parse_metadata` adds stays out of the text.
#[test]
fn a_parse_error_carries_no_context_line() {
    let report = verify_text(
        None,
        Path::new("PLAN-bad.md"),
        &plan_with_stage_field("bogus"),
    );

    let message = &report.errors[0].message;
    assert!(
        !message.contains("Failed to parse YAML metadata"),
        "{message}"
    );
    assert!(report.yaml_location.is_some(), "{report:?}");
}
