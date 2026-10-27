//! Plan validation mapped to the wire, and the cache's plan selection.

use std::path::Path;

use tempfile::TempDir;

use crate::plan_server::files::plan_view::plan_view;
use crate::plan_server::files::validate::{is_plan_file, validate_plan, NO_METADATA_NOTE};
use crate::plan_server::wire::files::MetadataView;
use crate::planning::sections::sections_of;

const PLAN: &str = "doc/plans/PLAN-x.md";

/// A plan of one stage; `extra` is appended to the stage's YAML lines.
pub(super) fn plan(working_dir: &str, acceptance: &str, extra: &str) -> String {
    format!(
        "# Plan X\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  stages:\n    \
         - id: stage-one\n      name: \"Stage One\"\n      stage_type: standard\n      \
         working_dir: \"{working_dir}\"\n      acceptance:\n        - \"{acceptance}\"\n\
         {extra}```\n\n<!-- END loom METADATA -->\n"
    )
}

pub(super) const SELF_DEPENDENCY: &str = "      dependencies:\n        - stage-one\n";

/// The 1-based number of the first line of `text` that equals `wanted` once trimmed.
fn line_of(text: &str, wanted: &str) -> u32 {
    let index = text.lines().position(|line| line.trim() == wanted).unwrap();
    u32::try_from(index + 1).unwrap()
}

fn root() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("doc/plans")).unwrap();
    root
}

#[test]
fn only_written_plan_files_are_validated() {
    assert!(is_plan_file(Path::new("doc/plans/PLAN-x.md")));
    for other in [
        "doc/plans/DONE-PLAN-x.md",
        "doc/plans/IN_PROGRESS-PLAN-x.md",
        "doc/plans/archive/PLAN-x.md",
        "doc/plans/briefs/x/PLAN-y.md",
        "doc/plans/PLAN-x.txt",
        "doc/product/section_01.md",
    ] {
        assert!(!is_plan_file(Path::new(other)), "{other}");
    }
}

#[test]
fn a_plan_without_metadata_is_valid_prose_with_a_note() {
    let root = root();
    let view = validate_plan(root.path(), Path::new(PLAN), "# Plan X\n\nJust prose.\n");
    assert!(view.valid);
    assert!(view.errors.is_empty());
    assert_eq!(view.metadata, MetadataView::Missing);
    assert_eq!(view.notes, vec![NO_METADATA_NOTE.to_owned()]);
}

#[test]
fn a_stage_error_points_at_the_stage_id_line() {
    let root = root();
    let text = plan(".", "true", SELF_DEPENDENCY);
    let view = validate_plan(root.path(), Path::new(PLAN), &text);
    assert!(!view.valid);
    assert_eq!(view.metadata, MetadataView::Present);
    let error = view
        .errors
        .iter()
        .find(|error| error.stage_id.as_deref() == Some("stage-one"))
        .expect("a stage error");
    assert_eq!(error.line, Some(line_of(&text, "- id: stage-one")));
}

#[test]
fn warnings_leave_a_plan_valid() {
    let root = root();
    let text = plan("loom", "loom/target/debug/loom --help", "");
    // The input checks need the plan itself on disk.
    std::fs::write(root.path().join(PLAN), &text).unwrap();
    let view = validate_plan(root.path(), Path::new(PLAN), &text);
    assert!(view.valid, "{view:?}");
    assert!(!view.warnings.structural.is_empty(), "{view:?}");
}

#[test]
fn a_yaml_error_is_located_in_the_file_and_matches_the_stages_panel() {
    let root = root();
    let text = plan(".", "true", "").replace("name: \"Stage One\"", "name: a: b");
    let view = validate_plan(root.path(), Path::new(PLAN), &text);
    assert!(!view.valid);
    assert_eq!(view.errors.len(), 1, "{view:?}");
    let error = &view.errors[0];
    assert!(error.message.starts_with("YAML parse error"), "{error:?}");
    assert_eq!(error.line, Some(line_of(&text, "name: a: b")));
    assert!(error.column.is_some_and(|column| column > 1), "{error:?}");
    let panel = plan_view(
        root.path(),
        Path::new(PLAN),
        &text,
        &sections_of(&text),
        None,
    );
    assert!(panel.stages.is_empty());
    let panel_error = panel
        .yaml_error
        .expect("the panel reports the same parse failure");
    assert_eq!(panel_error, error.message);
}
