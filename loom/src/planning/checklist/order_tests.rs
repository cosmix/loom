//! Offer order ("first follows document order") and the skipped-step scenarios of section 01.

use std::path::PathBuf;

use super::tests::{ids, load, metaplan, owned, plan, section, spec, vision, META};
use super::{CurrentStep, ItemStatus, Offer, Step, StepMark};

/// A second metaplan file; `doc/plans/index.md` sorts before it.
const SECOND_META: &str = "doc/plans/metaplan-b.md";

fn write_plan(metaplan: &str, slug: &str) -> Offer {
    Offer::WritePlan {
        metaplan: PathBuf::from(metaplan),
        slug: slug.into(),
    }
}

fn pressure_test(slug: &str) -> Offer {
    Offer::PressureTest { slug: slug.into() }
}

/// The metaplan fixture written to `rel` instead of `doc/plans/index.md`.
fn metaplan_at(rel: &str, slugs: &[&str]) -> (String, String) {
    (rel.to_string(), metaplan(slugs).1)
}

#[test]
fn first_missing_plan_follows_metaplan_heading_order() {
    let files = spec(vec![owned(metaplan(&["z", "a"]))]);
    let (_temp, project) = load(&files);
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::Step(Step::Plans));
    assert_eq!(c.primary, Some(write_plan(META, "z")));
    assert_eq!(c.secondary, [write_plan(META, "a")]);
}

#[test]
fn first_untested_plan_follows_metaplan_heading_order() {
    let files = spec(vec![
        owned(metaplan(&["z", "a"])),
        plan("doc/plans/PLAN-a.md", false),
        plan("doc/plans/PLAN-z.md", false),
    ]);
    let (_temp, project) = load(&files);
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::Step(Step::PressureTest));
    assert_eq!(c.primary, Some(pressure_test("z")));
    assert_eq!(ids(&c.secondary), ["start-run", "pressure-test:a"]);
}

#[test]
fn first_missing_plan_belongs_to_the_first_metaplan_in_reading_order() {
    let files = spec(vec![
        metaplan_at(SECOND_META, &["x"]),
        owned(metaplan(&["y"])),
    ]);
    let (_temp, project) = load(&files);
    let c = &project.checklist;
    assert_eq!(c.primary, Some(write_plan(META, "y")));
    assert_eq!(c.secondary, [write_plan(SECOND_META, "x")]);
    let listed: Vec<_> = project.checklist.state.metaplans.iter().collect();
    assert_eq!(listed, [&PathBuf::from(META), &PathBuf::from(SECOND_META)]);
}

#[test]
fn a_slug_named_by_two_metaplans_is_offered_once_for_the_first() {
    let files = spec(vec![
        metaplan_at(SECOND_META, &["a", "b"]),
        owned(metaplan(&["a"])),
    ]);
    let (_temp, project) = load(&files);
    let c = &project.checklist;
    assert_eq!(c.primary, Some(write_plan(META, "a")));
    assert_eq!(c.secondary, [write_plan(SECOND_META, "b")]);
}

#[test]
fn plans_without_a_product_spec_keep_their_gap_and_test_offers() {
    let files = vec![
        owned(metaplan(&["a", "b"])),
        plan("doc/plans/PLAN-a.md", false),
    ];
    let (_temp, project) = load(&files);
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::Step(Step::Vision));
    assert_eq!(c.primary, Some(Offer::StartVision));
    let want = [write_plan(META, "b"), pressure_test("a")];
    assert_eq!(c.secondary, want);
    let gap = c
        .items
        .iter()
        .find(|i| i.step == Step::Plans && i.status == ItemStatus::Gap);
    let gap = gap.expect("a gap item for the unwritten plan b");
    assert_eq!(gap.offer, Some(write_plan(META, "b")));
    assert_eq!(gap.path, Some(PathBuf::from("doc/plans/PLAN-b.md")));
}

#[test]
fn plans_without_a_metaplan_get_pressure_test_offers() {
    let files = spec(vec![plan("doc/plans/PLAN-z.md", false)]);
    let (_temp, project) = load(&files);
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::Step(Step::Metaplan));
    assert_eq!(c.primary, Some(Offer::StartMetaplan));
    assert_eq!(ids(&c.secondary), ["pressure-test:z"]);
}

#[test]
fn section_files_without_a_vision_are_unlisted_with_no_offer() {
    let (_temp, project) = load(&[section(1)]);
    let c = &project.checklist;
    assert_eq!(c.current, CurrentStep::Step(Step::Vision));
    assert_eq!(c.primary, Some(Offer::StartVision));
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
    assert_eq!(project.vision_rows, Ok(vec![]));
    assert!(
        project
            .problems
            .iter()
            .all(|p| p.code != "vision-table-unreadable"),
        "{:?}",
        project.problems
    );
    let c = &project.checklist;
    assert_eq!(c.steps[1], (Step::ProductSpec, StepMark::Done));
    assert_eq!(c.current, CurrentStep::Step(Step::Metaplan));
    assert_eq!(c.primary, Some(Offer::StartMetaplan));
    let mut all = ids(&c.secondary);
    all.extend(c.primary.iter().map(Offer::id));
    assert!(!all.contains(&"fix-vision-table".to_string()), "{all:?}");
    let step2_gap = c
        .items
        .iter()
        .any(|i| i.step == Step::ProductSpec && i.status == ItemStatus::Gap);
    assert!(!step2_gap, "{:?}", c.items);
    assert!(c
        .items
        .iter()
        .all(|i| i.offer != Some(Offer::FixVisionTable)));
    assert_eq!(c.state.sections_listed, 0);
}
