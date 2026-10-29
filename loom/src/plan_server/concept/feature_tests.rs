//! Feature isolation and feature-bearing concept payloads.

use std::fs;

use super::delta::{delta, snapshot};
use super::fixtures as fx;
use super::{load_page, PageContext, PageKey};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::watch::Batch;

fn two_features() -> fx::Fixture {
    fx::fixture(&[
        (fx::VISION, &fx::vision(&[1])),
        (fx::S1, "# Demo section\n\n## A\n\nDemo.\n"),
        ("doc/product/other/index.md", &fx::vision(&[1])),
        (
            "doc/product/other/section_01.md",
            "# Other section\n\n## A\n\nOther.\n",
        ),
    ])
}

#[test]
fn two_features_have_independent_pages_and_payload_features() {
    let fixture = two_features();
    let project = fx::load(&fixture.root);
    let eval = evaluate(&project, &[]);
    let ctx = PageContext::default();
    let load = |key| load_page(&project, &eval, &PageKey::parse(key).unwrap(), &ctx).unwrap();

    let demo = load("overview/demo/product");
    let other = load("overview/other/product");

    assert_eq!(demo.key_chips.len(), 1);
    assert_eq!(other.key_chips.len(), 1);
    assert_eq!(demo.key_chips[0].doc.id, "demo/product/section_01");
    assert_eq!(other.key_chips[0].doc.id, "other/product/section_01");
    assert_eq!(serde_json::to_value(&demo).unwrap()["feature"], "demo");
    assert_eq!(serde_json::to_value(&other).unwrap()["feature"], "other");
    let section = load("other/product/section_01");
    assert_eq!(section.feature.as_deref(), Some("other"));
    assert_eq!(
        section.path.as_deref(),
        Some("doc/product/other/section_01.md")
    );
}

#[test]
fn engineering_overview_lists_its_feature_without_standalone_plans() {
    let fixture = fx::fixture(&[
        (fx::META, &fx::metaplan(&["PLAN-demo-work.md"])),
        ("doc/plans/demo/PLAN-demo-work.md", "# Demo work\n"),
        (
            "doc/plans/other/index.md",
            &fx::metaplan(&["PLAN-other-work.md"]),
        ),
        ("doc/plans/other/PLAN-other-work.md", "# Other work\n"),
        ("doc/plans/PLAN-standalone.md", "# Standalone\n"),
    ]);
    let project = fx::load(&fixture.root);
    let eval = evaluate(&project, &[]);
    let ctx = PageContext::default();
    let key = PageKey::parse("overview/demo/engineering").unwrap();

    let page = load_page(&project, &eval, &key, &ctx).unwrap();

    let ids: Vec<_> = page
        .key_chips
        .iter()
        .map(|chip| chip.doc.id.as_str())
        .collect();
    assert_eq!(ids, ["plans/demo-work"]);
    assert!(page.unlisted.is_empty());
    let wrong = PageKey::parse("other/engineering/demo-work").unwrap();
    assert!(load_page(&project, &eval, &wrong, &ctx).is_none());
    let standalone = PageKey::parse("plans/standalone").unwrap();
    let standalone = load_page(&project, &eval, &standalone, &ctx).unwrap();
    assert!(standalone.key_chips.is_empty());
    assert_eq!(
        serde_json::to_value(standalone).unwrap()["feature"],
        serde_json::Value::Null
    );
}

#[test]
fn an_unknown_feature_has_no_overview() {
    let fixture = two_features();
    let project = fx::load(&fixture.root);
    let key = PageKey::parse("overview/unknown/product").unwrap();

    let page = load_page(
        &project,
        &evaluate(&project, &[]),
        &key,
        &PageContext::default(),
    );

    assert!(page.is_none());
}

#[test]
fn a_batch_emits_separate_feature_deltas() {
    let fixture = two_features();
    fx::seal_document(&fixture.root, fx::ID1, &[("Demo holds.", "a")]);
    fx::seal_document(
        &fixture.root,
        "other/product/section_01",
        &[("Other holds.", "a")],
    );
    let mut project = fx::load(&fixture.root);
    let ctx = PageContext::default();
    let before = snapshot(&project, &evaluate(&project, &[]), &ctx);
    fx::put(
        &fixture.root,
        fx::S1,
        "# Demo section\n\n## A\n\nEdited demo.\n",
    );
    fx::put(
        &fixture.root,
        "doc/product/other/section_01.md",
        "# Other section\n\n## A\n\nEdited other.\n",
    );
    let batch = Batch {
        changed: vec![fx::S1.into(), "doc/product/other/section_01.md".into()],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    let after = snapshot(&project, &evaluate(&project, &[]), &ctx);

    let changes = delta(&project, &before, &after, &batch);

    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].feature.as_deref(), Some("demo"));
    assert_eq!(
        changes[0].pages,
        ["demo/product/section_01", "overview/demo/product"]
    );
    assert_eq!(changes[0].out_of_date, [fx::ID1]);
    assert_eq!(changes[0].sources, [fx::S1]);
    assert_eq!(changes[1].feature.as_deref(), Some("other"));
    assert_eq!(
        changes[1].pages,
        ["other/product/section_01", "overview/other/product"]
    );
    assert_eq!(
        serde_json::to_value(&changes[1]).unwrap()["feature"],
        "other"
    );
}

#[test]
fn deleting_a_feature_keeps_its_old_page_keys_in_its_delta() {
    let fixture = two_features();
    let mut project = fx::load(&fixture.root);
    let ctx = PageContext::default();
    let before = snapshot(&project, &evaluate(&project, &[]), &ctx);
    fs::remove_file(fixture.root.join(fx::VISION)).unwrap();
    fs::remove_file(fixture.root.join(fx::S1)).unwrap();
    let batch = Batch {
        removed: vec![fx::VISION.into(), fx::S1.into()],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    let after = snapshot(&project, &evaluate(&project, &[]), &ctx);

    let changes = delta(&project, &before, &after, &batch);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].feature.as_deref(), Some("demo"));
    assert!(changes[0]
        .pages
        .iter()
        .any(|key| key == "demo/product/section_01"));
    assert!(changes[0].pages.iter().all(|key| !key.contains("other")));
}

#[test]
fn a_plan_move_invalidates_both_feature_keys() {
    let fixture = fx::fixture(&[
        (fx::META, &fx::metaplan(&["PLAN-work.md"])),
        ("doc/plans/demo/PLAN-work.md", "# Work\n"),
        ("doc/plans/other/index.md", &fx::metaplan(&["PLAN-work.md"])),
    ]);
    let mut project = fx::load(&fixture.root);
    let ctx = PageContext::default();
    let before = snapshot(&project, &evaluate(&project, &[]), &ctx);
    fs::rename(
        fixture.root.join("doc/plans/demo/PLAN-work.md"),
        fixture.root.join("doc/plans/other/PLAN-work.md"),
    )
    .unwrap();
    let batch = Batch {
        renamed: vec![(
            "doc/plans/demo/PLAN-work.md".into(),
            "doc/plans/other/PLAN-work.md".into(),
        )],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    let after = snapshot(&project, &evaluate(&project, &[]), &ctx);

    let changes = delta(&project, &before, &after, &batch);

    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].feature.as_deref(), Some("demo"));
    assert!(changes[0]
        .pages
        .iter()
        .any(|key| key == "demo/engineering/work"));
    assert_eq!(changes[1].feature.as_deref(), Some("other"));
    assert!(changes[1]
        .pages
        .iter()
        .any(|key| key == "other/engineering/work"));
}
