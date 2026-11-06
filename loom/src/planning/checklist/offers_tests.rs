//! Offer wire contracts, feature targets and draft-all placement.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tempfile::TempDir;

use super::tests::{self, load, metaplan, owned, plan, vision};
use super::{offers::move_offers, Checklist, Offer};
use crate::planning::discover::discover;

fn assert_offer(offer: Offer, id: &str, label: &str, target: Option<&str>, expected: Value) {
    assert_eq!(offer.id(), id);
    assert_eq!(offer.label(), label);
    assert_eq!(offer.feature(), expected["feature"].as_str());
    assert_eq!(offer.target(), target.map(PathBuf::from));
    assert_eq!(serde_json::to_value(offer).unwrap(), expected);
}

#[test]
fn start_vision_contract() {
    assert_offer(
        Offer::StartVision {
            feature: "demo".into(),
        },
        "start-vision",
        "Start the vision",
        None,
        json!({"id": "start-vision", "kind": "start-vision",
            "feature": "demo", "target": null}),
    );
}

#[test]
fn fix_vision_table_contract() {
    assert_offer(
        Offer::FixVisionTable {
            feature: "demo".into(),
        },
        "fix-vision-table",
        "Fix the section table",
        Some("doc/product/demo/index.md"),
        json!({"id": "fix-vision-table", "kind": "fix-vision-table",
            "feature": "demo", "target": "doc/product/demo/index.md"}),
    );
}

#[test]
fn write_section_contract() {
    assert_offer(
        Offer::WriteSection {
            feature: "demo".into(),
            number: 7,
        },
        "write-section:07",
        "Write section 07",
        Some("doc/product/demo/section_07.md"),
        json!({"id": "write-section:07", "kind": "write-section", "feature": "demo",
            "number": 7, "target": "doc/product/demo/section_07.md"}),
    );
}

#[test]
fn add_section_to_vision_contract() {
    assert_offer(
        Offer::AddSectionToVision {
            feature: "demo".into(),
            number: 7,
        },
        "add-section-to-vision:07",
        "Add section 07 to the vision",
        Some("doc/product/demo/section_07.md"),
        json!({"id": "add-section-to-vision:07", "kind": "add-section-to-vision",
            "feature": "demo", "number": 7, "target": "doc/product/demo/section_07.md"}),
    );
}

#[test]
fn start_metaplan_contract() {
    assert_offer(
        Offer::StartMetaplan {
            feature: "demo".into(),
        },
        "start-metaplan",
        "Start the metaplan",
        None,
        json!({"id": "start-metaplan", "kind": "start-metaplan",
            "feature": "demo", "target": null}),
    );
}

#[test]
fn write_plan_contract() {
    assert_offer(
        Offer::WritePlan {
            feature: "demo".into(),
            metaplan: "doc/plans/demo/index.md".into(),
            slug: "alpha".into(),
        },
        "write-plan:alpha",
        "Write plan alpha",
        Some("doc/plans/demo/PLAN-alpha.md"),
        json!({"id": "write-plan:alpha", "kind": "write-plan", "feature": "demo",
            "metaplan": "doc/plans/demo/index.md", "slug": "alpha",
            "target": "doc/plans/demo/PLAN-alpha.md"}),
    );
}

#[test]
fn pressure_test_contract() {
    assert_offer(
        Offer::PressureTest {
            feature: "demo".into(),
            slug: "alpha".into(),
        },
        "pressure-test:alpha",
        "Pressure-test alpha",
        Some("doc/plans/demo/PLAN-alpha.md"),
        json!({"id": "pressure-test:alpha", "kind": "pressure-test", "feature": "demo",
            "slug": "alpha", "target": "doc/plans/demo/PLAN-alpha.md"}),
    );
}

#[test]
fn shared_action_contracts() {
    assert_offer(
        Offer::StartRun,
        "start-run",
        "Start a run",
        None,
        json!({"id": "start-run", "kind": "start-run", "target": null}),
    );
    assert_offer(
        Offer::PlanFollowUps,
        "plan-follow-ups",
        "Plan follow-ups",
        None,
        json!({"id": "plan-follow-ups", "kind": "plan-follow-ups", "target": null}),
    );
}

#[test]
fn draft_all_sections_contract() {
    assert_offer(
        Offer::DraftAllSections {
            feature: "demo".into(),
        },
        "draft-all-sections",
        "Draft all with a team",
        None,
        json!({"id": "draft-all-sections", "kind": "draft-all-sections",
            "feature": "demo", "target": null}),
    );
}

#[test]
fn draft_all_plans_contract() {
    assert_offer(
        Offer::DraftAllPlans {
            feature: "demo".into(),
        },
        "draft-all-plans",
        "Draft all with a team",
        None,
        json!({"id": "draft-all-plans", "kind": "draft-all-plans",
            "feature": "demo", "target": null}),
    );
}

#[test]
fn move_into_feature_contract() {
    assert_offer(
        Offer::MoveIntoFeature {
            path: "doc/product/section_07.md".into(),
        },
        "move-into-feature:doc/product/section_07.md",
        "Move doc/product/section_07.md into a feature",
        Some("doc/product/section_07.md"),
        json!({"id": "move-into-feature:doc/product/section_07.md", "kind": "move-into-feature",
            "path": "doc/product/section_07.md", "target": "doc/product/section_07.md"}),
    );
}

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn move_offers_include_only_misplaced_documents_in_path_order() {
    let temp = TempDir::new().unwrap();
    for path in [
        "doc/product/section_07.md",
        "doc/product/index.md",
        "doc/plans/index.md",
        "doc/plans/PLAN-standalone.md",
        "doc/plans/archive/DONE-PLAN-finished.md",
        "doc/product/demo/index.md",
        "doc/product/demo/section_01.md",
        "doc/product/demo/notes.md",
        "doc/plans/demo/index.md",
        "doc/plans/demo/metaplan-02.md",
        "doc/plans/demo/PLAN-scoped.md",
        "doc/other.md",
    ] {
        put(temp.path(), path, "# Document\n");
    }
    let mut tree = discover(temp.path()).unwrap();
    // The public helper sorts its result even if a caller supplies an unsorted tree.
    tree.docs.reverse();

    let offers = move_offers(&tree);

    assert_eq!(
        offers.iter().map(Offer::id).collect::<Vec<_>>(),
        [
            "move-into-feature:doc/plans/index.md",
            "move-into-feature:doc/product/index.md",
            "move-into-feature:doc/product/section_07.md",
        ]
    );
}

fn checklist(numbers: &[u32], slugs: &[&str], written: &[&str]) -> Checklist {
    let mut files = vec![owned(vision(numbers)), owned(metaplan(slugs))];
    files.extend(written.iter().map(|path| plan(path, false)));
    let (_temp, project) = load(&files);
    tests::checklist(&project).clone()
}

fn ids(checklist: &Checklist) -> Vec<String> {
    checklist
        .primary
        .iter()
        .chain(&checklist.secondary)
        .map(Offer::id)
        .collect()
}

#[test]
fn draft_all_sections_follows_first_missing_section_in_table_order() {
    let checklist = checklist(&[7, 3, 9], &[], &[]);

    assert_eq!(
        ids(&checklist),
        [
            "write-section:07",
            "draft-all-sections",
            "write-section:03",
            "write-section:09",
        ]
    );
    assert_eq!(
        checklist.secondary[0],
        Offer::DraftAllSections {
            feature: "demo".into()
        }
    );
}

#[test]
fn draft_all_sections_requires_two_missing_sections() {
    let one = checklist(&[7, 3], &[], &["doc/product/demo/section_07.md"]);
    let none = checklist(&[7], &[], &["doc/product/demo/section_07.md"]);

    assert_eq!(ids(&one), ["write-section:03"]);
    assert!(!ids(&none).iter().any(|id| id == "draft-all-sections"));
}

#[test]
fn draft_all_plans_follows_first_missing_plan_in_metaplan_order() {
    let checklist = checklist(&[], &["zeta", "alpha", "beta"], &[]);

    assert_eq!(
        ids(&checklist),
        [
            "write-plan:zeta",
            "draft-all-plans",
            "write-plan:alpha",
            "write-plan:beta",
        ]
    );
    assert_eq!(
        checklist.secondary[0],
        Offer::DraftAllPlans {
            feature: "demo".into()
        }
    );
}

#[test]
fn draft_all_plans_requires_two_missing_plans() {
    let one = checklist(&[], &["zeta", "alpha"], &["doc/plans/demo/PLAN-zeta.md"]);
    let none = checklist(&[], &["zeta"], &["doc/plans/demo/PLAN-zeta.md"]);

    assert_eq!(ids(&one), ["write-plan:alpha", "pressure-test:zeta"]);
    assert!(!ids(&none).iter().any(|id| id == "draft-all-plans"));
}

#[test]
fn both_draft_all_offers_stay_beside_their_guided_offer() {
    let checklist = checklist(&[7, 3], &["zeta", "alpha"], &[]);

    assert_eq!(
        ids(&checklist),
        [
            "write-section:07",
            "draft-all-sections",
            "write-section:03",
            "write-plan:zeta",
            "draft-all-plans",
            "write-plan:alpha",
        ]
    );
}

#[test]
fn repeated_plan_entries_do_not_offer_a_team_for_one_missing_plan() {
    let checklist = checklist(&[], &["alpha", "alpha"], &[]);

    assert_eq!(ids(&checklist), ["write-plan:alpha"]);
}

#[test]
fn unlisted_sections_keep_add_offers_and_unlisted_plans_have_no_add_offer() {
    let checklist = checklist(
        &[1],
        &["alpha"],
        &[
            "doc/product/demo/section_07.md",
            "doc/plans/demo/PLAN-unlisted.md",
        ],
    );

    assert_eq!(
        ids(&checklist),
        [
            "write-section:01",
            "write-plan:alpha",
            "pressure-test:unlisted",
            "add-section-to-vision:07",
        ]
    );
}

#[test]
fn review_feature_contract() {
    assert_offer(
        Offer::ReviewFeature,
        "review-feature",
        "Review the feature",
        None,
        json!({"id": "review-feature", "kind": "review-feature", "target": null}),
    );
}

#[test]
fn review_feature_is_a_secondary_once_a_plan_is_running_or_done() {
    for path in [
        "doc/plans/demo/IN_PROGRESS-PLAN-one.md",
        "doc/plans/demo/archive/DONE-PLAN-one.md",
    ] {
        let checklist = checklist(&[], &["one"], &[path]);
        assert!(
            checklist.secondary.contains(&Offer::ReviewFeature),
            "{path}"
        );
        assert_ne!(checklist.primary, Some(Offer::ReviewFeature));
    }
}

#[test]
fn review_feature_is_not_offered_before_a_plan_runs() {
    for written in [&["doc/plans/demo/PLAN-one.md"][..], &[]] {
        let checklist = checklist(&[], &["one"], written);
        assert!(!ids(&checklist).iter().any(|id| id == "review-feature"));
    }
}

#[test]
fn review_feature_is_offered_once_with_several_runs() {
    let running = "doc/plans/demo/IN_PROGRESS-PLAN-one.md";
    let done = "doc/plans/demo/archive/DONE-PLAN-two.md";
    let checklist = checklist(&[], &["one", "two"], &[running, done]);

    let reviews = ids(&checklist)
        .into_iter()
        .filter(|id| id == "review-feature");
    assert_eq!(reviews.count(), 1);
}
