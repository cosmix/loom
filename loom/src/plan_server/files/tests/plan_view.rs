use std::fs;
use std::path::Path;

use crate::plan_server::files::plan_view::plan_view;
use crate::plan_server::wire::files::PlanStateView;
use crate::planning::sections::sections_of;

const PLAN: &str = "# Plan\nImplements: section_1, section_02\n\n<!-- loom METADATA\n```yaml\nloom:\n  version: 1\n  stages:\n    - id: build\n      name: Build\n      working_dir: .\n      description: Make it\n      acceptance:\n        - cargo test\n      files: [src/lib.rs]\n```\n<!-- END loom METADATA -->\n\n## Pressure tests\n- 2025-02-02: first\n- 2024-01-01: older\n";

#[test]
fn exposes_parsed_stages_links_and_lines() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("doc/product")).unwrap();
    fs::create_dir_all(temp.path().join("doc/plans")).unwrap();
    fs::write(temp.path().join("doc/product/section_1.md"), "# One").unwrap();
    let rel = Path::new("doc/plans/IN_PROGRESS-PLAN-demo.md");
    fs::write(temp.path().join(rel), PLAN).unwrap();
    let view = plan_view(temp.path(), rel, PLAN, &sections_of(PLAN), None);
    assert_eq!(view.slug, "demo");
    assert_eq!(view.state, PlanStateView::Running);
    assert_eq!(
        view.implements[0].path.as_deref(),
        Some("doc/product/section_1.md")
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
    let view = plan_view(temp.path(), rel, invalid, &sections_of(invalid), None);
    assert!(view.stages.is_empty());
    assert!(view
        .yaml_error
        .as_deref()
        .unwrap()
        .starts_with("YAML parse error: "));
    assert_eq!(view.metadata_line, Some(1));
    let prose = "# Draft\n";
    let view = plan_view(temp.path(), rel, prose, &sections_of(prose), None);
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
    let view = plan_view(temp.path(), rel, PLAN, &sections_of(PLAN), None);
    assert!(view.last_change.is_none());
}
