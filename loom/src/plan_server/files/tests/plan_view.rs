use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::plan_server::files::plan_view::{latest_pressure_date, plan_view, product_sections};
use crate::plan_server::wire::files::PlanStateView;
use crate::planning::discover::discover;
use crate::planning::sections::sections_of;

const PLAN: &str = "# Plan\nImplements: section_1, section_02\n\n<!-- loom METADATA\n```yaml\nloom:\n  version: 1\n  stages:\n    - id: build\n      name: Build\n      working_dir: .\n      description: Make it\n      acceptance:\n        - cargo test\n      files: [src/lib.rs]\n```\n<!-- END loom METADATA -->\n\n## Pressure tests\n- 2025-02-02: first\n- 2024-01-01: older\n";

#[test]
fn exposes_parsed_stages_links_and_lines() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("doc/product/demo")).unwrap();
    fs::create_dir_all(temp.path().join("doc/plans/demo")).unwrap();
    fs::write(temp.path().join("doc/product/demo/section_1.md"), "# One").unwrap();
    let rel = Path::new("doc/plans/demo/IN_PROGRESS-PLAN-demo.md");
    fs::write(temp.path().join(rel), PLAN).unwrap();
    let sections = product_sections(&discover(temp.path()).unwrap(), Some("demo"));
    let view = plan_view(temp.path(), rel, PLAN, &sections_of(PLAN), &sections, None);
    assert_eq!(view.slug, "demo");
    assert_eq!(view.state, PlanStateView::Running);
    assert_eq!(
        view.implements[0].path.as_deref(),
        Some("doc/product/demo/section_1.md")
    );
    assert_eq!(view.implements[1].path, None);
    assert_eq!(view.pressure_date.as_deref(), Some("2025-02-02"));
    assert!(view.last_change.as_deref().unwrap().ends_with('Z'));
    assert_eq!(view.metadata_line, Some(4));
    assert_eq!(view.stages.len(), 1);
    let stage = &view.stages[0];
    assert_eq!(stage.line, Some(9));
    assert_eq!(stage.description, "Make it");
    assert_eq!(stage.acceptance, ["cargo test"]);
    assert_eq!(stage.files, ["src/lib.rs"]);
    assert!(view.yaml.as_ref().unwrap().starts_with("loom:"));
    assert!(view.yaml_error.is_none());
}

#[test]
fn parse_errors_and_missing_metadata_leave_no_stages() {
    let temp = tempfile::tempdir().unwrap();
    let rel = Path::new("doc/plans/PLAN-demo.md");
    let invalid = "<!-- loom METADATA\n```yaml\nloom: [\n```\n<!-- END loom METADATA -->\n";
    let none = BTreeMap::new();
    let view = plan_view(
        temp.path(),
        rel,
        invalid,
        &sections_of(invalid),
        &none,
        None,
    );
    assert!(view.stages.is_empty());
    assert!(view
        .yaml_error
        .as_deref()
        .unwrap()
        .starts_with("YAML parse error: "));
    assert_eq!(view.metadata_line, Some(1));
    let prose = "# Draft\n";
    let view = plan_view(temp.path(), rel, prose, &sections_of(prose), &none, None);
    assert!(view.yaml.is_none());
    assert!(view.yaml_error.is_none());
    assert!(view.metadata_line.is_none());
}

#[test]
fn modification_time_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("doc/plans")).unwrap();
    fs::write(temp.path().join("target"), PLAN).unwrap();
    let rel = Path::new("doc/plans/PLAN-demo.md");
    symlink(temp.path().join("target"), temp.path().join(rel)).unwrap();
    let none = BTreeMap::new();
    let view = plan_view(temp.path(), rel, PLAN, &sections_of(PLAN), &none, None);
    assert!(view.last_change.is_none());
}

/// Pressure records in document order, neither first nor last the latest.
pub(super) const OUT_OF_ORDER_PRESSURE: &str =
    "# Plan\n\n## Pressure tests\n\n- 2025-01-01: a\n- 2025-03-03: b\n- 2025-02-02: c\n";

#[test]
fn the_pressure_date_is_the_latest_record_not_the_last() {
    assert_eq!(
        latest_pressure_date(OUT_OF_ORDER_PRESSURE).as_deref(),
        Some("2025-03-03")
    );
    let rel = Path::new("doc/plans/PLAN-demo.md");
    let text = OUT_OF_ORDER_PRESSURE;
    let none = BTreeMap::new();
    let view = plan_view(
        Path::new("/nonexistent"),
        rel,
        text,
        &sections_of(text),
        &none,
        None,
    );
    assert_eq!(view.pressure_date.as_deref(), Some("2025-03-03"));
}

#[test]
fn implements_links_resolve_against_the_given_sections_without_walking_the_disk() {
    let rel = Path::new("doc/plans/PLAN-demo.md");
    let text = "# Plan\nImplements: section_07\n";
    let sections = BTreeMap::from([(7, "doc/product/section_07.md".to_owned())]);
    // The root does not exist: a link can only come from `sections`.
    let view = plan_view(
        Path::new("/nonexistent"),
        rel,
        text,
        &sections_of(text),
        &sections,
        None,
    );
    assert_eq!(view.implements.len(), 1);
    assert_eq!(view.implements[0].number, 7);
    assert_eq!(
        view.implements[0].path.as_deref(),
        Some("doc/product/section_07.md")
    );
}

#[test]
fn product_links_are_scoped_to_the_plans_feature() {
    let temp = tempfile::tempdir().unwrap();
    for feature in ["alpha", "beta"] {
        let path = temp
            .path()
            .join(format!("doc/product/{feature}/section_01.md"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "# One\n").unwrap();
    }
    let tree = discover(temp.path()).unwrap();
    assert_eq!(
        product_sections(&tree, Some("beta")),
        BTreeMap::from([(1, "doc/product/beta/section_01.md".to_owned())])
    );
    assert!(product_sections(&tree, None).is_empty());
}
