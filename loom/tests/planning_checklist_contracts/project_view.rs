use super::*;
use loom::planning::briefs::{orphan_bundles, plan_briefs};
use loom::planning::discover::{discover, read_doc};
use loom::planning::kinds::DocId;
use loom::planning::sections::sections_of;
use loom::planning::watch::{OwnWrites, Watcher};
use loom::planning::xref::{references, RefForm};

fn assert_same(got: &Project, want: &Project) {
    assert_eq!(got.docs, want.docs, "docs");
    assert_eq!(got.tree, want.tree, "tree");
    assert_eq!(got.features, want.features, "features");
    assert_eq!(got.standalone, want.standalone, "standalone");
    assert_eq!(got.moves, want.moves, "moves");
    assert_eq!(got.implements, want.implements, "implements");
    assert_eq!(got.locked, want.locked, "locked");
    assert_eq!(got.problems, want.problems, "problems");
}

#[test]
fn refresh_matches_load() {
    let a = ("doc/plans/demo/PLAN-a.md", plan("A", "section_01"));
    let b = ("doc/plans/demo/PLAN-b.md", plan("B", "section_02"));
    let (_temp, root) = tree(&spec(vec![metaplan(&["a", "b"]), a, b]));
    let mut project = Project::load(&root).unwrap();
    let mut watcher = Watcher::new(root.clone(), OwnWrites::default());
    let initial = watcher.poll().unwrap().expect("initial batch");
    project.refresh(&initial).unwrap();
    assert_same(&project, &Project::load(&root).unwrap());

    let (product, plans) = (root.join("doc/product/demo"), root.join("doc/plans/demo"));
    fs::remove_file(product.join("section_03.md")).unwrap();
    fs::rename(product.join("section_02.md"), product.join("section_04.md")).unwrap();
    fs::rename(plans.join("PLAN-a.md"), plans.join("IN_PROGRESS-PLAN-a.md")).unwrap();
    let (meta, text) = metaplan(&["a", "b", "c"]);
    put(&root, meta, text);
    for i in 0..26 {
        let note = format!("doc/product/demo/note_{i:02}.md");
        put(&root, &note, format!("# Note {i}\n"));
    }
    let batch = (0..4).find_map(|_| watcher.poll().unwrap());
    let batch = batch.expect("a quiet walk delivers the batch");
    assert!(!batch.initial);
    project.refresh(&batch).unwrap();
    assert_same(&project, &Project::load(&root).unwrap());
}

#[test]
fn brief_bundle_grouping() {
    let briefs = ["doc/plans/demo", "briefs"].join("/");
    let bundle = format!("{briefs}/p");
    let outside = format!("{briefs}/orphan/shared.md");
    let missing = format!("{bundle}/beta/missing.md");
    let text = format!("# Plan: P\n\nImplements: none\n\nStage beta: `{outside}`, `{missing}`.\n");
    let (_temp, root) = tree(&[("doc/plans/demo/PLAN-p.md", text)]);
    let files = [
        "common.md",
        "alpha/w1-a.md",
        "gamma/notes.md",
        "mockup.json",
    ];
    for rel in files {
        put(&root, &format!("{bundle}/{rel}"), "# Brief\n");
    }
    put(&root, &outside, "# Shared\n");
    let tree = discover(&root).unwrap();
    let entry = tree.plan("p").expect("plan p");
    let stages = ["alpha".to_string(), "beta".to_string()];
    let got = plan_briefs(&root, entry, &stages).unwrap();
    let in_bundle = |rel: &str| p(&format!("{bundle}/{rel}"));
    let alpha = got.by_stage.get("alpha").cloned().unwrap_or_default();
    assert_eq!(alpha, [in_bundle("alpha/w1-a.md")], "{got:?}");
    let beta = got.by_stage.get("beta");
    assert!(!beta.is_some_and(|files| !files.is_empty()), "{got:?}");
    assert_eq!(got.common, [in_bundle("common.md")], "{got:?}");
    let mut other = got.other.clone();
    other.sort();
    let mut want = vec![
        in_bundle("gamma/notes.md"),
        in_bundle("mockup.json"),
        p(&outside),
    ];
    want.sort();
    assert_eq!(other, want, "{got:?}");
    assert_eq!(got.broken.len(), 1, "{:?}", got.broken);
    assert!(got.broken[0].contains("missing.md"), "{:?}", got.broken);
    assert_eq!(orphan_bundles(&tree), ["orphan"]);
}

#[test]
fn cross_reference_forms() {
    let rules = "# Section 01\n\n## Detection rules\n\nRules.\n".to_string();
    // Canonical top-level tokens resolve by the repository-wide plan slug.
    let text = "# Section 02\n\nSee [all](section_01.md), [rules](section_01.md#detection-rules) \
                and [gone](section_01.md#gone).\n\n\
                The plan `doc/plans/PLAN-a.md` was `DONE-PLAN-a.md` in a draft.\n\
                Each plan lives at `doc/plans/demo/PLAN-<slug>.md`.\n\
                Code in [main](../../../loom/src/main.rs), docs at [site](https://example.com/x).\n";
    let rel = "doc/product/demo/section_02.md";
    let (_temp, root) = tree(&[
        ("doc/product/demo/section_01.md", rules),
        (
            "doc/plans/demo/IN_PROGRESS-PLAN-a.md",
            plan("A", "section_01"),
        ),
        (rel, text.to_string()),
    ]);
    let tree = discover(&root).unwrap();
    let lookup = |path: &Path| read_doc(&root, path).ok().map(|t| sections_of(&t));
    let refs = references(Path::new(rel), text, &tree, &lookup);
    let s1 = DocId("product/demo/section_01".to_string());
    let links = refs.iter().filter(|r| r.form == RefForm::RelativeLink);
    let links: Vec<_> = links.filter_map(|r| r.target.clone().map(Some)).collect();
    let anchored = Some((s1.clone(), Some("detection-rules".to_string())));
    assert_eq!(links, [Some((s1, None)), anchored], "{refs:?}");
    let plan_a = Some((DocId("plan/a".to_string()), None));
    for form in [RefForm::PathToken, RefForm::PlanFile] {
        let found: Vec<_> = refs.iter().filter(|r| r.form == form).collect();
        assert_eq!(found.len(), 1, "{refs:?}");
        assert_eq!(found[0].target, plan_a, "{refs:?}");
    }
    let broken = refs.iter().filter(|r| r.target.is_none());
    let broken: Vec<&str> = broken.map(|r| r.raw.as_str()).collect();
    assert_eq!(broken.len(), 1, "{refs:?}");
    assert!(broken[0].contains("#gone"), "{refs:?}");
    for skipped in ["main.rs", "https:", "<slug>"] {
        let found = refs.iter().any(|r| r.raw.contains(skipped));
        assert!(!found, "{skipped}: {refs:?}");
    }
}
