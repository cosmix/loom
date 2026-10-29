//! Feature checklist conversion and snapshot contract tests.

use super::*;
use serde_json::{json, Value};
use std::path::Path;

fn put(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn assert_offer(offer: Offer, expected: Value) {
    let view = offer_view(&offer);
    assert_eq!(serde_json::to_value(&view).unwrap(), expected);
    assert_eq!(serde_json::from_value::<OfferView>(expected).unwrap(), view);
}

#[test]
fn checklist_fixture_matches_serde_output() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/checklist.json")).unwrap();
    let view: ChecklistView = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(view).unwrap(), fixture);
}

#[test]
fn feature_checklist_converts_with_the_core_offer_fields() {
    let temp = tempfile::tempdir().unwrap();
    put(
        temp.path(),
        "doc/product/demo/index.md",
        "# Vision\n\n## Specification sections\n\n| # | Name |\n| --- | --- |\n| 05 | Example |\n",
    );
    let project = Project::load(temp.path()).unwrap();
    let view = ChecklistView::from(&project.feature("demo").unwrap().checklist);

    assert_eq!(view.feature, "demo");
    assert_eq!(view.current, CurrentView::Step { number: 2 });
    assert_eq!(view.primary.as_ref().unwrap().kind, OfferKind::WriteSection);
    assert_eq!(
        serde_json::to_value(&view.primary).unwrap(),
        json!({
            "id": "write-section:05", "kind": "write_section", "label": "Write section 05",
            "feature": "demo", "number": 5, "target": "doc/product/demo/section_05.md"
        })
    );
    assert!(view.items.iter().any(|item| item.path.as_deref()
        == Some("doc/product/demo/section_05.md")
        && item.offer == view.primary));
    assert_eq!(view.steps.len(), 7);
}

#[test]
fn draft_all_offers_include_labels_and_features() {
    let cases = [
        (
            Offer::DraftAllSections {
                feature: "demo".into(),
            },
            json!({
                "id": "draft-all-sections", "kind": "draft_all_sections", "feature": "demo",
                "label": "Draft all missing sections with a team", "target": null
            }),
        ),
        (
            Offer::DraftAllPlans {
                feature: "demo".into(),
            },
            json!({
                "id": "draft-all-plans", "kind": "draft_all_plans", "feature": "demo",
                "label": "Draft all missing plans with a team", "target": null
            }),
        ),
    ];
    for (offer, expected) in cases {
        assert_offer(offer, expected);
    }
}

#[test]
fn plan_offers_include_labels_and_variant_fields() {
    let cases = [
        (
            Offer::WritePlan {
                feature: "demo".into(),
                slug: "fix".into(),
                metaplan: "doc/plans/demo/index.md".into(),
            },
            json!({
                "id": "write-plan:fix", "kind": "write_plan", "label": "Write plan fix",
                "feature": "demo", "slug": "fix",
                "metaplan": "doc/plans/demo/index.md", "target": "doc/plans/demo/PLAN-fix.md"
            }),
        ),
        (
            Offer::PressureTest {
                feature: "demo".into(),
                slug: "fix".into(),
            },
            json!({
                "id": "pressure-test:fix", "kind": "pressure_test", "label": "Pressure-test fix",
                "feature": "demo", "slug": "fix", "target": "doc/plans/demo/PLAN-fix.md"
            }),
        ),
    ];
    for (offer, expected) in cases {
        assert_offer(offer, expected);
    }
}

#[test]
fn vision_and_metaplan_offers_keep_their_labels_and_core_targets() {
    let cases = [
        (
            Offer::StartVision {
                feature: "demo".into(),
            },
            json!({
                "id": "start-vision", "kind": "start_vision", "feature": "demo",
                "label": "Start a vision discussion", "target": null
            }),
        ),
        (
            Offer::FixVisionTable {
                feature: "demo".into(),
            },
            json!({
                "id": "fix-vision-table", "kind": "fix_vision_table", "feature": "demo",
                "label": "Fix the vision's section table", "target": "doc/product/demo/index.md"
            }),
        ),
        (
            Offer::StartMetaplan {
                feature: "demo".into(),
            },
            json!({
                "id": "start-metaplan", "kind": "start_metaplan", "feature": "demo",
                "label": "Start a metaplan from the product spec", "target": null
            }),
        ),
    ];
    for (offer, expected) in cases {
        assert_offer(offer, expected);
    }
}

#[test]
fn section_offers_pad_labels_without_truncating_large_numbers() {
    let cases = [
        (
            Offer::AddSectionToVision {
                feature: "demo".into(),
                number: 5,
            },
            json!({
                "id": "add-section-to-vision:05", "kind": "add_section_to_vision",
                "label": "Add section 05 to the vision's table", "feature": "demo",
                "number": 5, "target": "doc/product/demo/section_05.md"
            }),
        ),
        (
            Offer::WriteSection {
                feature: "demo".into(),
                number: 105,
            },
            json!({
                "id": "write-section:105", "kind": "write_section", "label": "Write section 105",
                "feature": "demo", "number": 105, "target": "doc/product/demo/section_105.md"
            }),
        ),
    ];
    for (offer, expected) in cases {
        assert_offer(offer, expected);
    }
}

#[test]
fn unscoped_offers_omit_optional_context_and_keep_a_null_target() {
    let cases = [
        (
            Offer::StartRun,
            json!({
                "id": "start-run", "kind": "start_run", "label": "Start a run", "target": null
            }),
        ),
        (
            Offer::RefineSpec,
            json!({
                "id": "refine-spec", "kind": "refine_spec",
                "label": "Refine the spec in light of what was built", "target": null
            }),
        ),
        (
            Offer::NewMetaplanRound,
            json!({
                "id": "new-metaplan-round", "kind": "new_metaplan_round",
                "label": "Start a new metaplan round", "target": null
            }),
        ),
    ];
    for (offer, expected) in cases {
        assert_offer(offer, expected);
    }
}

#[test]
fn project_snapshot_groups_features_and_uses_winning_standalone_paths_and_titles() {
    let temp = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("doc/product/zeta/index.md", "# Zeta\n"),
        ("doc/plans/alpha/index.md", "# Alpha\n"),
        ("doc/plans/PLAN-fix.md", "# Stale fix\n"),
        ("doc/plans/IN_PROGRESS-PLAN-fix.md", "# Fix in progress\n"),
        ("doc/plans/PLAN-first.md", "# First\n"),
        ("doc/plans/archive/DONE-PLAN-last.md", "# Last\n"),
        ("doc/plans/archive/RETIRED-PLAN-old.md", "# Old\n"),
        ("doc/plans/alpha/PLAN-owned.md", "# Owned\n"),
        ("doc/product/index.md", "# Misplaced\n"),
    ] {
        put(temp.path(), path, text);
    }
    let project = Project::load(temp.path()).unwrap();
    let snapshot = ProjectChecklistView::from(&project);
    let names: Vec<_> = snapshot
        .checklists
        .iter()
        .map(|view| view.feature.as_str())
        .collect();
    assert_eq!(names, ["alpha", "zeta"]);
    assert_eq!(
        serde_json::to_value(snapshot.standalone).unwrap(),
        json!([
            {"slug":"first","path":"doc/plans/PLAN-first.md","state":"written","title":"First"},
            {"slug":"fix","path":"doc/plans/IN_PROGRESS-PLAN-fix.md","state":"running","title":"Fix in progress"},
            {"slug":"last","path":"doc/plans/archive/DONE-PLAN-last.md","state":"done","title":"Last"},
            {"slug":"old","path":"doc/plans/archive/RETIRED-PLAN-old.md","state":"retired","title":"Old"}
        ])
    );
    assert_eq!(
        serde_json::to_value(snapshot.moves).unwrap(),
        json!([{
            "id":"move-into-feature:doc/product/index.md", "kind":"move_into_feature",
            "label":"Move doc/product/index.md into a feature", "target":"doc/product/index.md"
        }])
    );
}

#[test]
fn standalone_title_and_move_changes_affect_snapshot_equality() {
    let temp = tempfile::tempdir().unwrap();
    put(temp.path(), "doc/plans/PLAN-fix.md", "# Before\n");
    let before = ProjectChecklistView::from(&Project::load(temp.path()).unwrap());
    put(temp.path(), "doc/plans/PLAN-fix.md", "# After\n");
    let renamed = ProjectChecklistView::from(&Project::load(temp.path()).unwrap());
    assert_eq!(before.checklists, renamed.checklists);
    assert_ne!(before, renamed);
    put(temp.path(), "doc/product/index.md", "# Misplaced\n");
    let misplaced = ProjectChecklistView::from(&Project::load(temp.path()).unwrap());
    assert_eq!(renamed.checklists, misplaced.checklists);
    assert_ne!(renamed, misplaced);
}

#[test]
fn rejects_unknown_fields() {
    assert!(serde_json::from_value::<OfferView>(json!({
        "id":"start-run", "kind":"start_run", "label":"Start a run",
        "target":null, "extra":true
    }))
    .is_err());
    assert!(serde_json::from_value::<CurrentView>(
        json!({"kind":"step", "number":2, "extra":true})
    )
    .is_err());
    assert!(serde_json::from_value::<ItemView>(
        json!({"step":1,"text":"x","path":null,"offer":null,"extra":true})
    )
    .is_err());
    assert!(serde_json::from_value::<ProjectChecklistView>(
        json!({"checklists":[],"standalone":[],"moves":[],"checklist":null})
    )
    .is_err());
}

#[test]
fn offer_rejects_core_kind_spelling_and_removed_wire_kind() {
    for kind in ["start-run", "add_plan_to_metaplan"] {
        assert!(serde_json::from_value::<OfferView>(json!({
            "id":"x", "kind":kind, "label":"x", "target":null
        }))
        .is_err());
    }
}

#[test]
fn offer_requires_a_human_label() {
    assert!(serde_json::from_value::<OfferView>(json!({
        "id":"start-run", "kind":"start_run", "target":null
    }))
    .is_err());
}

#[cfg(unix)]
#[test]
fn move_offer_uses_lossy_text_for_non_utf8_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let offer = Offer::MoveIntoFeature {
        path: OsString::from_vec(b"doc/product/section_\xff.md".to_vec()).into(),
    };

    assert_offer(
        offer,
        json!({
            "id":"move-into-feature:doc/product/section_�.md", "kind":"move_into_feature",
            "label":"Move doc/product/section_�.md into a feature",
            "target":"doc/product/section_�.md"
        }),
    );
}
