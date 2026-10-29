//! Navigation tree grouping and ordering.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::plan_view::OUT_OF_ORDER_PRESSURE;
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::files::tree::build_tree;
use crate::plan_server::wire::checklist::{ChecklistView, OfferKind};
use crate::plan_server::wire::files::EntryKind;
use crate::planning::project::Project;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_fixture(root: &Path) {
    write(root, "doc/product/demo/index.md", "# Vision\n\n## Specification sections\n\n| Number | Name |\n| --- | --- |\n| 2 | Second |\n| 1 | Missing |\n");
    write(root, "doc/product/demo/section_02.md", "# Second title\n");
    write(root, "doc/product/demo/section_03.md", "# Third title\n");
    write(
        root,
        "doc/plans/demo/metaplan.md",
        "# Roadmap\n\n## 1. `PLAN-alpha.md`\n\n## 2. `PLAN-missing.md`\n",
    );
    write(
        root,
        "doc/plans/demo/PLAN-alpha.md",
        "# Alpha\n\n## Pressure tests\n\n- 2025-01-02: first\n- 2025-02-03: last\n",
    );
    write(root, "doc/plans/demo/briefs/alpha/common.md", "# Common\n");
    write(
        root,
        "doc/plans/demo/briefs/alpha/stage/b.md",
        "# Stage brief\n",
    );
    write(root, "doc/plans/demo/briefs/orphan/common.md", "# Orphan\n");
    write(
        root,
        "doc/plans/demo/archive/REVIEW-PLAN-old.md",
        "# Review\n",
    );
    write(root, "doc/loom/knowledge/INDEX.md", "# Index\n\n## Tier 2 topics\n\n### architecture\n\n| Topic | Blurb | Lines |\n| --- | --- | --- |\n| [overview](architecture/overview.md) | Overview blurb | 4 |\n");
    write(
        root,
        "doc/loom/knowledge/architecture.md",
        "# Architecture\n",
    );
    write(
        root,
        "doc/loom/knowledge/architecture/overview.md",
        "# Overview\n",
    );
    write(root, "doc/misc.md", "no heading\n");
}

#[test]
fn groups_documents_and_preserves_listed_order_and_gaps() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_fixture(root);
    let project = Project::load(root).unwrap();
    let texts = TextStore::load(root, &project).unwrap();
    let tree = build_tree(
        &project,
        &texts,
        &HashMap::from([("doc/plans/demo/PLAN-alpha.md".into(), 1)]),
    );
    assert_eq!(
        tree.product[0]
            .children
            .iter()
            .map(|e| e.title.as_str())
            .collect::<Vec<_>>(),
        ["Vision", "Second title", "Missing", "Third title"]
    );
    assert!(tree.product[0].children[2].gap);
    let meta = &tree.plans[0].children[0];
    assert_eq!(meta.kind, EntryKind::Metaplan);
    let plan = &meta.children[0];
    assert_eq!(plan.title, "Alpha");
    assert_eq!(plan.pressure_date.as_deref(), Some("2025-02-03"));
    assert_eq!(plan.errors, 1);
    assert_eq!(plan.children[0].title, "Common");
    assert_eq!(plan.children[1].kind, EntryKind::BriefFolder);
    assert_eq!(plan.children[1].children[0].title, "Stage brief");
    assert!(meta.children[1].gap);
    assert_eq!(tree.knowledge[0].title, "Index");
    assert_eq!(tree.knowledge[1].title, "Architecture");
    assert_eq!(
        tree.knowledge[2].children[0].blurb.as_deref(),
        Some("Overview blurb")
    );
    assert!(tree.archive[0]
        .children
        .iter()
        .any(|e| e.kind == EntryKind::Review));
    assert!(tree.archive[0]
        .children
        .iter()
        .any(|e| e.title == "orphan" && e.children[0].title == "Orphan"));
    assert!(tree.other.iter().any(|e| e.title == "misc.md"));
}

#[test]
fn section_gap_target_matches_its_checklist_item_path() {
    let temp = tempfile::tempdir().unwrap();
    write_fixture(temp.path());
    let project = Project::load(temp.path()).unwrap();
    let texts = TextStore::load(temp.path(), &project).unwrap();
    let checklist = ChecklistView::from(&project.feature("demo").unwrap().checklist);

    let tree = build_tree(&project, &texts, &HashMap::new());
    let gap = &tree.product[0].children[2];
    let item = checklist
        .items
        .iter()
        .find(|item| {
            item.offer.as_ref().is_some_and(|offer| {
                offer.kind == OfferKind::WriteSection && offer.number == Some(1)
            })
        })
        .unwrap();

    assert!(gap.gap);
    assert_eq!(
        gap.target.as_deref(),
        Some("doc/product/demo/section_01.md")
    );
    assert_eq!(gap.target, item.path);
    assert_eq!(tree.product[0].target, None);
    assert_eq!(tree.product[0].children[0].target, None);
}

#[test]
fn metaplan_gap_target_matches_its_checklist_item_path() {
    let temp = tempfile::tempdir().unwrap();
    write_fixture(temp.path());
    let project = Project::load(temp.path()).unwrap();
    let texts = TextStore::load(temp.path(), &project).unwrap();
    let checklist = ChecklistView::from(&project.feature("demo").unwrap().checklist);

    let tree = build_tree(&project, &texts, &HashMap::new());
    let gap = &tree.plans[0].children[0].children[1];
    let item = checklist
        .items
        .iter()
        .find(|item| {
            item.offer.as_ref().is_some_and(|offer| {
                offer.kind == OfferKind::WritePlan && offer.slug.as_deref() == Some("missing")
            })
        })
        .unwrap();

    assert!(gap.gap);
    assert_eq!(
        gap.target.as_deref(),
        Some("doc/plans/demo/PLAN-missing.md")
    );
    assert_eq!(gap.target, item.path);
    assert_eq!(tree.plans[0].children[0].children[0].target, None);
}

#[test]
fn a_plan_row_shows_the_latest_pressure_date_from_the_stored_text() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, "doc/plans/PLAN-alpha.md", OUT_OF_ORDER_PRESSURE);
    let project = Project::load(root).unwrap();
    let texts = TextStore::load(root, &project).unwrap();
    // The tree takes the text from the store: the file is gone from disk.
    fs::remove_file(root.join("doc/plans/PLAN-alpha.md")).unwrap();
    let tree = build_tree(&project, &texts, &HashMap::new());
    let plan = tree
        .plans
        .iter()
        .find(|entry| entry.path.as_deref() == Some("doc/plans/PLAN-alpha.md"))
        .expect("the plan row");
    assert_eq!(plan.pressure_date.as_deref(), Some("2025-03-03"));
    assert_eq!(plan.errors, 0);
}

fn write_separate_shelves_fixture(root: &Path) {
    for feature in ["alpha", "beta"] {
        write(
            root,
            &format!("doc/product/{feature}/index.md"),
            "# Vision\n",
        );
        write(
            root,
            &format!("doc/product/{feature}/section_01.md"),
            "# One\n",
        );
        write(
            root,
            &format!("doc/plans/{feature}/PLAN-{feature}.md"),
            "# Plan\n",
        );
    }
    write(root, "doc/plans/PLAN-solo.md", "# Solo\n");
    write(root, "doc/product/index.md", "# Misplaced vision\n");
    write(root, "doc/product/section_01.md", "# Misplaced section\n");
    write(root, "doc/plans/index.md", "# Misplaced metaplan\n");
}

#[test]
fn two_features_and_a_standalone_plan_keep_separate_shelves() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_separate_shelves_fixture(root);
    let project = Project::load(root).unwrap();
    let texts = TextStore::load(root, &project).unwrap();
    let tree = build_tree(&project, &texts, &HashMap::new());

    for (index, feature) in ["alpha", "beta"].iter().enumerate() {
        for group in [&tree.product[index], &tree.plans[index]] {
            assert_eq!(group.kind, EntryKind::Feature);
            assert_eq!(group.feature.as_deref(), Some(*feature));
            assert!(group
                .children
                .iter()
                .all(|doc| doc.feature.as_deref() == Some(*feature)));
            assert!(group.children.iter().all(|doc| !doc.misplaced));
        }
    }
    let solo = tree
        .plans
        .iter()
        .find(|entry| entry.title == "Solo")
        .unwrap();
    assert_eq!(solo.feature, None);
    assert!(!solo.misplaced);
    let misplaced: Vec<_> = tree
        .product
        .iter()
        .chain(&tree.plans)
        .filter(|entry| entry.misplaced)
        .collect();
    assert_eq!(misplaced.len(), 3);
    assert!(misplaced.iter().all(|entry| entry.feature.is_none()));
    let json = serde_json::to_value(&tree).unwrap();
    assert_eq!(json["product"][0]["feature"], "alpha");
    assert_eq!(json["product"][0]["children"][0]["misplaced"], false);
}

#[test]
fn an_archived_feature_plan_keeps_its_home_briefs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, "doc/plans/demo/archive/PLAN-older.md", "# Older\n");
    write(
        root,
        "doc/plans/demo/briefs/older/common.md",
        "# Feature brief\n",
    );
    write(
        root,
        "doc/plans/briefs/older/common.md",
        "# Standalone orphan\n",
    );
    let project = Project::load(root).unwrap();
    let texts = TextStore::load(root, &project).unwrap();
    let tree = build_tree(&project, &texts, &HashMap::new());
    let group = &tree.archive[0];
    assert_eq!(group.feature.as_deref(), Some("demo"));
    let plan = &group.children[0];
    assert_eq!(
        plan.path.as_deref(),
        Some("doc/plans/demo/archive/PLAN-older.md")
    );
    assert_eq!(plan.children[0].title, "Feature brief");
    assert_eq!(plan.children[0].feature.as_deref(), Some("demo"));
    assert_eq!(tree.archive[1].feature, None);
    assert_eq!(tree.archive[1].children[0].title, "Standalone orphan");
}
