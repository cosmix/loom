//! Feature checklist conversion and snapshot contract tests.

use super::*;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use std::fs;

#[path = "checklist_plan_rows_tests.rs"]
mod plan_rows;
use plan_rows::{assert_offer, pitch_of, put};

#[test]
fn pitch_joins_only_the_first_paragraph_after_the_title_and_keeps_inline_markdown() {
    let pitch = pitch_of(
        "Preamble is ignored.\n\n# Vision\n\nBuild **clear** plans with `loom`\nfor _every_ team.\n\nA later paragraph.\n",
    );

    assert_eq!(
        pitch.as_deref(),
        Some("Build **clear** plans with `loom` for _every_ team.")
    );
}

#[test]
fn pitch_skips_headings_lists_tables_fences_and_blockquotes() {
    let pitch = pitch_of(concat!(
        "# Vision\n\n## Overview\n\n- An unordered item\nits continuation\n\n",
        "* Another item\n\n+ Another item\n\n1. An ordered item\n\n2) Another item\n\n",
        "| Name | Value |\n| --- | --- |\n| Demo | Yes |\n\n",
        "Name | Value\n--- | ---\nDemo | Yes\n\n",
        "```rust\nignored code\n\nstill code\n```\n\n~~~\nmore code\n~~~\n\n",
        "> A quote\nits continuation\n\n    indented code\n\n",
        "A subheading\n---\n\nThe feature keeps planning work easy to follow.\n",
    ));

    assert_eq!(
        pitch.as_deref(),
        Some("The feature keeps planning work easy to follow.")
    );
}

#[test]
fn pitch_caps_at_280_characters_including_the_ellipsis_without_splitting_unicode() {
    let exact = pitch_of(&format!("# Vision\n\n{}\n", "é".repeat(280))).unwrap();
    assert_eq!(exact, "é".repeat(280));
    let cut = pitch_of(&format!("# Vision\n\n{}\n", "é".repeat(281))).unwrap();
    assert_eq!(cut.chars().count(), 280);
    assert_eq!(cut, format!("{}…", "é".repeat(279)));
}

#[test]
fn pitch_is_absent_without_a_heading_or_a_prose_paragraph() {
    assert_eq!(pitch_of("No level-one heading.\n"), None);
    assert_eq!(pitch_of("# Vision\n\n- A list only\n"), None);
    assert_eq!(
        pitch_of("# Vision\n\n```\nUnclosed code\n\nMore code\n"),
        None
    );
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
    assert_eq!(view.steps.len(), 6);
    assert_eq!(
        (view.steps[5].number, view.steps[5].name.as_str()),
        (6, "Run")
    );
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
                "label": "Draft all with a team", "target": null
            }),
        ),
        (
            Offer::DraftAllPlans {
                feature: "demo".into(),
            },
            json!({
                "id": "draft-all-plans", "kind": "draft_all_plans", "feature": "demo",
                "label": "Draft all with a team", "target": null
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
                "label": "Start the vision", "target": null
            }),
        ),
        (
            Offer::FixVisionTable {
                feature: "demo".into(),
            },
            json!({
                "id": "fix-vision-table", "kind": "fix_vision_table", "feature": "demo",
                "label": "Fix the section table", "target": "doc/product/demo/index.md"
            }),
        ),
        (
            Offer::StartMetaplan {
                feature: "demo".into(),
            },
            json!({
                "id": "start-metaplan", "kind": "start_metaplan", "feature": "demo",
                "label": "Start the metaplan", "target": null
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
                "label": "Add section 05 to the vision", "feature": "demo",
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
            Offer::PlanFollowUps,
            json!({
                "id": "plan-follow-ups", "kind": "plan_follow_ups",
                "label": "Plan follow-ups", "target": null
            }),
        ),
        (
            Offer::ReviewFeature,
            json!({
                "id": "review-feature", "kind": "review_feature",
                "label": "Review the feature", "target": null
            }),
        ),
    ];
    for (offer, expected) in cases {
        assert_offer(offer, expected);
    }
}

/// Each plan carries its file's modification time; checks it and returns the rest of the rows.
fn standalone_without_last_change(
    snapshot: &ProjectChecklistView,
    root: &std::path::Path,
) -> serde_json::Value {
    let mut standalone = serde_json::to_value(&snapshot.standalone).unwrap();
    for row in standalone.as_array_mut().unwrap() {
        let modified = row.as_object_mut().unwrap().remove("last_change").unwrap();
        let path = root.join(row["path"].as_str().unwrap());
        let mtime = DateTime::<Utc>::from(fs::metadata(path).unwrap().modified().unwrap());
        assert_eq!(
            modified,
            json!(mtime.to_rfc3339_opts(SecondsFormat::Secs, true))
        );
    }
    standalone
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
    let snapshot = ProjectChecklistView::from(&Project::load(temp.path()).unwrap());
    let names: Vec<_> = snapshot
        .checklists
        .iter()
        .map(|view| view.feature.as_str())
        .collect();
    assert_eq!(names, ["alpha", "zeta"]);
    // Alpha has only a metaplan; Zeta's vision has no paragraph.
    assert_eq!(snapshot.checklists[0].pitch, None);
    assert_eq!(snapshot.checklists[1].pitch, None);
    let standalone = standalone_without_last_change(&snapshot, temp.path());
    assert_eq!(
        standalone,
        json!([
            {"slug":"first","path":"doc/plans/PLAN-first.md","state":"written","title":"First","has_summary":false},
            {"slug":"fix","path":"doc/plans/IN_PROGRESS-PLAN-fix.md","state":"running","title":"Fix in progress","has_summary":false},
            {"slug":"last","path":"doc/plans/archive/DONE-PLAN-last.md","state":"done","title":"Last","has_summary":false},
            {"slug":"old","path":"doc/plans/archive/RETIRED-PLAN-old.md","state":"retired","title":"Old","has_summary":false}
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
