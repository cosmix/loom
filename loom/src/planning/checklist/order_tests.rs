//! Offer order ("first follows document order") and the skipped-step scenarios of section 01.

use std::path::PathBuf;

use super::tests::{
    checklist, ids, load, metaplan, owned, plan, section, spec, vision, FEATURE, META,
};
use super::{CurrentStep, ItemStatus, Offer, Step, StepMark};

fn write_plan(metaplan: &str, slug: &str) -> Offer {
    Offer::WritePlan {
        feature: FEATURE.into(),
        metaplan: PathBuf::from(metaplan),
        slug: slug.into(),
    }
}

fn pressure_test(slug: &str) -> Offer {
    Offer::PressureTest {
        feature: FEATURE.into(),
        slug: slug.into(),
    }
}

fn draft_all_plans() -> Offer {
    Offer::DraftAllPlans {
        feature: FEATURE.into(),
    }
}

/// The metaplan fixture with `follow_ups` entries under a `## Follow-ups` heading.
fn metaplan_with_follow_ups(slugs: &[&str], follow_ups: &[&str]) -> (String, String) {
    let mut text = metaplan(slugs).1;
    text.push_str("## Follow-ups\n\n");
    for (i, slug) in follow_ups.iter().enumerate() {
        let number = slugs.len() + i + 1;
        text.push_str(&format!("## {number}. F (`PLAN-{slug}.md`)\n\n"));
    }
    (META.to_string(), text)
}

#[test]
fn first_missing_plan_follows_metaplan_heading_order() {
    let files = spec(vec![owned(metaplan(&["z", "a"]))]);
    let (_temp, project) = load(&files);
    let c = checklist(&project);
    assert_eq!(c.current, CurrentStep::Step(Step::Plans));
    assert_eq!(c.primary, Some(write_plan(META, "z")));
    assert_eq!(c.secondary, [draft_all_plans(), write_plan(META, "a")]);
}

#[test]
fn first_untested_plan_follows_metaplan_heading_order() {
    let files = spec(vec![
        owned(metaplan(&["z", "a"])),
        plan("doc/plans/demo/PLAN-a.md", false),
        plan("doc/plans/demo/PLAN-z.md", false),
    ]);
    let (_temp, project) = load(&files);
    let c = checklist(&project);
    assert_eq!(c.current, CurrentStep::Step(Step::PressureTest));
    assert_eq!(c.primary, Some(pressure_test("z")));
    assert_eq!(ids(&c.secondary), ["start-run", "pressure-test:a"]);
}

#[test]
fn finished_feature_has_no_primary_and_keeps_followups_in_order() {
    let files = spec(vec![
        owned(metaplan(&["a"])),
        plan("doc/plans/demo/archive/DONE-PLAN-a.md", true),
    ]);
    let (_temp, project) = load(&files);
    let c = checklist(&project);
    assert_eq!(c.current, CurrentStep::ReadyAllDone);
    assert_eq!(c.primary, None);
    assert_eq!(c.secondary, [Offer::PlanFollowUps, Offer::ReviewFeature]);
    assert_eq!(c.steps.len(), 6);
    assert_eq!(c.steps[5], (Step::Run, StepMark::Done));
}

#[test]
fn follow_up_entries_come_after_the_numbered_ones() {
    let files = spec(vec![metaplan_with_follow_ups(&["y"], &["x"])]);
    let (_temp, project) = load(&files);
    let c = checklist(&project);
    assert_eq!(c.primary, Some(write_plan(META, "y")));
    assert_eq!(c.secondary, [draft_all_plans(), write_plan(META, "x")]);
    let listed: Vec<_> = c.state.metaplans.iter().collect();
    assert_eq!(listed, [&PathBuf::from(META)]);
}

#[test]
fn a_slug_named_twice_in_the_metaplan_is_offered_once() {
    let files = spec(vec![metaplan_with_follow_ups(&["a"], &["a", "b"])]);
    let (_temp, project) = load(&files);
    let c = checklist(&project);
    assert_eq!(c.primary, Some(write_plan(META, "a")));
    assert_eq!(c.secondary, [draft_all_plans(), write_plan(META, "b")]);
}

#[test]
fn plans_without_a_product_spec_keep_their_gap_and_test_offers() {
    let files = vec![
        owned(metaplan(&["a", "b"])),
        plan("doc/plans/demo/PLAN-a.md", false),
    ];
    let (_temp, project) = load(&files);
    let c = checklist(&project);
    assert_eq!(c.current, CurrentStep::Step(Step::Vision));
    assert_eq!(
        c.primary,
        Some(Offer::StartVision {
            feature: FEATURE.into(),
        })
    );
    let want = [write_plan(META, "b"), pressure_test("a")];
    assert_eq!(c.secondary, want);
    let gap = c
        .items
        .iter()
        .find(|i| i.step == Step::Plans && i.status == ItemStatus::Gap);
    let gap = gap.expect("a gap item for the unwritten plan b");
    assert_eq!(gap.offer, Some(write_plan(META, "b")));
    assert_eq!(gap.path, Some(PathBuf::from("doc/plans/demo/PLAN-b.md")));
}

#[test]
fn plans_without_a_metaplan_get_pressure_test_offers() {
    let files = spec(vec![plan("doc/plans/demo/PLAN-z.md", false)]);
    let (_temp, project) = load(&files);
    let c = checklist(&project);
    assert_eq!(c.current, CurrentStep::Step(Step::Metaplan));
    assert_eq!(
        c.primary,
        Some(Offer::StartMetaplan {
            feature: FEATURE.into(),
        })
    );
    assert_eq!(ids(&c.secondary), ["pressure-test:z"]);
}

#[test]
fn section_files_without_a_vision_are_unlisted_with_no_offer() {
    let (_temp, project) = load(&[section(1)]);
    let c = checklist(&project);
    assert_eq!(c.current, CurrentStep::Step(Step::Vision));
    assert_eq!(
        c.primary,
        Some(Offer::StartVision {
            feature: FEATURE.into(),
        })
    );
    let mut all = ids(&c.secondary);
    all.extend(c.primary.iter().map(Offer::id));
    assert!(
        !all.iter().any(|id| id.starts_with("add-section-to-vision")),
        "{all:?}"
    );
    let unlisted = c.items.iter().find(|i| i.status == ItemStatus::Unlisted);
    let unlisted = unlisted.expect("an unlisted section item");
    assert_eq!(unlisted.step, Step::ProductSpec);
    assert_eq!(unlisted.offer, None);
}

#[test]
fn a_vision_listing_no_sections_is_the_files_own_no_product_spec() {
    let (rel, text) = vision(&[]);
    let (_temp, project) = load(&[(rel.to_string(), text)]);
    assert_eq!(project.feature(FEATURE).unwrap().vision_rows, Ok(vec![]));
    assert!(
        project
            .problems
            .iter()
            .all(|p| p.code != "vision-table-unreadable"),
        "{:?}",
        project.problems
    );
    let c = checklist(&project);
    assert_eq!(c.steps[1], (Step::ProductSpec, StepMark::Done));
    assert_eq!(c.current, CurrentStep::Step(Step::Metaplan));
    assert_eq!(
        c.primary,
        Some(Offer::StartMetaplan {
            feature: FEATURE.into(),
        })
    );
    let mut all = ids(&c.secondary);
    all.extend(c.primary.iter().map(Offer::id));
    assert!(!all.contains(&"fix-vision-table".to_string()), "{all:?}");
    let step2_gap = c
        .items
        .iter()
        .any(|i| i.step == Step::ProductSpec && i.status == ItemStatus::Gap);
    assert!(!step2_gap, "{:?}", c.items);
    assert!(c.items.iter().all(|i| i.offer
        != Some(Offer::FixVisionTable {
            feature: FEATURE.into(),
        })));
    assert_eq!(c.state.sections_listed, 0);
}
