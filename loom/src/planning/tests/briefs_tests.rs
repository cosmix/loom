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
