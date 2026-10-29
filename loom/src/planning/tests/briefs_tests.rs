//! Tests for `crate::planning::briefs`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::planning::briefs::{orphan_bundles, plan_briefs};
use crate::planning::discover::discover;

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn groups_bundle_inputs_and_finds_orphans() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let briefs = ["doc/plans", "briefs"].join("/");
    let bundle = format!("{briefs}/p");
    let outside = format!("{briefs}/orphan/shared.md");
    put(
        root,
        "doc/plans/PLAN-p.md",
        &format!("# Plan\n`{outside}`\n"),
    );
    for name in ["common.md", "alpha/w1.md", "gamma/notes.md", "mockup.json"] {
        put(root, &format!("{bundle}/{name}"), "# Brief\n");
    }
    put(root, &outside, "# Shared\n");
    let tree = discover(root).unwrap();
    let got = plan_briefs(
        root,
        tree.plan("p").unwrap(),
        &["alpha".into(), "beta".into()],
    )
    .unwrap();
    let path = |name: &str| PathBuf::from(format!("{bundle}/{name}"));
    assert_eq!(got.by_stage.get("alpha").unwrap(), &[path("alpha/w1.md")]);
    assert!(!got.by_stage.contains_key("beta"));
    assert_eq!(got.common, [path("common.md")]);
    assert_eq!(
        got.other,
        [
            PathBuf::from(outside),
            path("gamma/notes.md"),
            path("mockup.json")
        ]
    );
    assert!(got.broken.is_empty());
    assert_eq!(orphan_bundles(&tree), ["orphan"]);
}

#[test]
fn missing_input_is_reported_as_broken() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let briefs = ["doc/plans", "briefs"].join("/");
    let missing = format!("{briefs}/p/beta/missing.md");
    put(
        root,
        "doc/plans/PLAN-p.md",
        &format!("# Plan\n`{missing}`\n"),
    );
    let tree = discover(root).unwrap();
    let got = plan_briefs(root, tree.plan("p").unwrap(), &["beta".into()]).unwrap();
    assert_eq!(got.broken.len(), 1);
    assert!(got.broken[0].contains(&missing));
    assert!(got.broken[0].contains("does not exist"));
    assert!(got.by_stage.is_empty());
}

#[test]
fn feature_plan_resolves_its_own_bundle_including_after_archiving() {
    for plan in [
        "doc/plans/feature/PLAN-p.md",
        "doc/plans/feature/archive/DONE-PLAN-p.md",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let bundle = ["doc/plans/feature", "briefs", "p"].join("/");
        put(root, plan, "# Plan\n");
        for name in ["common.md", "alpha/w1.md", "gamma/notes.md"] {
            put(root, &format!("{bundle}/{name}"), "# Brief\n");
        }
        let standalone = ["doc/plans", "briefs", "p", "common.md"].join("/");
        put(root, &standalone, "# Standalone\n");

        let tree = discover(root).unwrap();
        let got = plan_briefs(root, tree.plan("p").unwrap(), &["alpha".into()]).unwrap();
        let path = |name: &str| PathBuf::from(format!("{bundle}/{name}"));
        assert_eq!(got.common, [path("common.md")], "{plan}");
        assert_eq!(got.by_stage["alpha"], [path("alpha/w1.md")], "{plan}");
        assert_eq!(got.other, [path("gamma/notes.md")], "{plan}");
        assert!(got.broken.is_empty(), "{plan}: {:?}", got.broken);
    }
}

#[test]
fn missing_feature_input_reports_its_feature_path_and_origin() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let missing = ["doc/plans/feature", "briefs/p/beta/missing.md"].join("/");
    let plan = "doc/plans/feature/PLAN-p.md";
    put(root, plan, &format!("# Plan\n`{missing}`\n"));

    let tree = discover(root).unwrap();
    let got = plan_briefs(root, tree.plan("p").unwrap(), &["beta".into()]).unwrap();
    assert_eq!(
        got.broken,
        [format!(
            "{missing} (referenced in {plan}): input does not exist"
        )]
    );
    assert!(got.by_stage.is_empty());
}

#[test]
fn orphan_bundles_include_feature_homes_and_ignore_invalid_names() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(root, "doc/plans/feature/PLAN-p.md", "# Plan\n");
    for bundle in [
        "doc/plans/feature/briefs/p",
        "doc/plans/feature/briefs/orphan",
        "doc/plans/other/briefs/another-orphan",
        "doc/plans/bad_name/briefs/invalid",
    ] {
        put(root, &format!("{bundle}/common.md"), "# Brief\n");
    }

    let tree = discover(root).unwrap();
    assert_eq!(orphan_bundles(&tree), ["another-orphan", "orphan"]);
}
