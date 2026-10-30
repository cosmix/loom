//! Feature boundaries for checklist marks, items and starting-point facts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::tests::{checklist, metaplan, owned, plan, section, spec, vision};
use super::{CurrentStep, ItemStatus, Offer, Step, StepMark};
use crate::planning::project::Project;
use crate::planning::slug::PlanState;

pub(super) fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

pub(super) fn tree(files: &[(&str, String)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("doc")).unwrap();
    for (rel, text) in files {
        put(temp.path(), rel, text);
    }
    temp
}

pub(super) fn load(files: &[(String, String)]) -> (TempDir, Project) {
    let pairs: Vec<(&str, String)> = files.iter().map(|(r, t)| (r.as_str(), t.clone())).collect();
    let temp = tree(&pairs);
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

#[test]
fn run_is_optional_until_every_entry_is_finished() {
    let files = vec![
        owned(metaplan(&["a", "b"])),
        plan("doc/plans/demo/archive/DONE-PLAN-a.md", true),
        plan("doc/plans/demo/PLAN-b.md", true),
    ];
    let (_temp, project) = load(&spec(files));
    assert_eq!(
        checklist(&project).steps[5],
        (Step::Run, StepMark::Optional)
    );
    assert_eq!(checklist(&project).current, CurrentStep::Ready);
}

#[test]
fn a_features_vision_does_not_supply_another_features_vision() {
    let files = [
        owned(vision(&[1])),
        ("doc/product/other/section_01.md".into(), section(1).1),
    ];

    let (_temp, project) = load(&files);
    let other = &project.feature("other").unwrap().checklist;

    assert_eq!(other.current, CurrentStep::Step(Step::Vision));
    assert!(!other.state.vision_present);
    assert_eq!(
        other.items[0].path,
        Some(PathBuf::from("doc/product/other/index.md"))
    );
    assert_eq!(
        other.primary,
        Some(Offer::StartVision {
            feature: "other".into()
        })
    );
}

#[test]
fn features_get_independent_checklists() {
    let mut files = spec(vec![
        owned(metaplan(&["a"])),
        plan("doc/plans/demo/PLAN-a.md", true),
    ]);
    let (_alone_temp, alone) = load(&files);
    files.extend([
        ("doc/product/other/index.md".into(), vision(&[1]).1),
        ("doc/product/other/section_02.md".into(), section(2).1),
        ("doc/plans/other/index.md".into(), metaplan(&["b"]).1),
        plan("doc/plans/other/PLAN-b.md", false),
    ]);

    let (_temp, project) = load(&files);
    let other = &project.feature("other").unwrap().checklist;

    assert_eq!(checklist(&project), checklist(&alone));
    assert_eq!(other.feature, "other");
    assert_eq!(other.current, CurrentStep::Step(Step::ProductSpec));
    assert_eq!(other.state.sections_written, [2]);
    assert_eq!(
        other.state.metaplans,
        [PathBuf::from("doc/plans/other/index.md")]
    );
    assert_eq!(
        other.state.plans_by_state,
        BTreeMap::from([(PlanState::Written, 1)])
    );
    let sections: Vec<_> = other
        .items
        .iter()
        .filter(|item| item.step == Step::ProductSpec)
        .map(|item| (item.path.clone(), item.status.clone()))
        .collect();
    assert_eq!(
        sections,
        [
            (
                Some(PathBuf::from("doc/product/other/section_01.md")),
                ItemStatus::Gap
            ),
            (
                Some(PathBuf::from("doc/product/other/section_02.md")),
                ItemStatus::Unlisted
            ),
        ]
    );
}

#[test]
fn standalone_and_other_feature_plans_do_not_change_marks() {
    let mut files = spec(vec![
        owned(metaplan(&["a"])),
        plan("doc/plans/demo/archive/DONE-PLAN-a.md", true),
    ]);
    let (_alone_temp, alone) = load(&files);
    files.extend([
        plan("doc/plans/IN_PROGRESS-PLAN-running.md", false),
        plan("doc/plans/other/IN_PROGRESS-PLAN-other-running.md", false),
        plan("doc/plans/PLAN-written.md", false),
        plan("doc/plans/other/PLAN-other-written.md", false),
    ]);

    let (_temp, project) = load(&files);
    let c = checklist(&project);
    let before = checklist(&alone);

    assert_eq!(c.current, CurrentStep::ReadyAllDone);
    assert_eq!(c.steps, before.steps);
    assert_eq!(c.items, before.items);
    assert_eq!(
        (&c.primary, &c.secondary),
        (&before.primary, &before.secondary)
    );
    assert_eq!(c.state.plans_unlisted, 0);
    assert_eq!(
        c.state.plans_by_state,
        BTreeMap::from([(PlanState::Done, 1)])
    );
    // Locked files are those under the feature's own directories.
    assert_eq!(
        c.state.locked_files,
        [PathBuf::from("doc/plans/demo/archive")]
    );
}

#[test]
fn foreign_finished_plans_satisfy_entries_and_report_the_wrong_home() {
    let files = spec(vec![
        owned(metaplan(&["standalone", "foreign"])),
        plan("doc/plans/archive/DONE-PLAN-standalone.md", true),
        plan("doc/plans/other/archive/DONE-PLAN-foreign.md", true),
    ]);

    let (_temp, project) = load(&files);
    let c = checklist(&project);

    assert_eq!(c.current, CurrentStep::ReadyAllDone);
    assert_eq!(c.steps[3], (Step::Plans, StepMark::Done));
    assert_eq!(c.steps[5], (Step::Run, StepMark::Done));
    assert!(c.state.plans_by_state.is_empty());
    let plans: Vec<_> = c
        .items
        .iter()
        .filter(|item| item.step == Step::Plans)
        .map(|item| (item.path.clone(), item.status.clone()))
        .collect();
    assert_eq!(
        plans,
        [
            (
                Some(PathBuf::from("doc/plans/archive/DONE-PLAN-standalone.md")),
                ItemStatus::Present
            ),
            (
                Some(PathBuf::from(
                    "doc/plans/other/archive/DONE-PLAN-foreign.md"
                )),
                ItemStatus::Present
            ),
        ]
    );
    assert_no_plan_write_offers(c);
    assert!(project
        .problems
        .iter()
        .any(|problem| problem.code == "metaplan-entry-foreign-plan"));
}

#[test]
fn foreign_written_plans_do_not_create_pressure_test_items() {
    let files = spec(vec![
        owned(metaplan(&["standalone", "foreign"])),
        plan("doc/plans/PLAN-standalone.md", false),
        plan("doc/plans/other/PLAN-foreign.md", false),
    ]);

    let (_temp, project) = load(&files);
    let c = checklist(&project);

    assert_eq!(c.current, CurrentStep::Ready);
    assert_eq!(c.steps[3], (Step::Plans, StepMark::Done));
    assert_eq!(c.steps[4], (Step::PressureTest, StepMark::Done));
    assert!(c.items.iter().all(|item| item.step != Step::PressureTest));
    assert!(c
        .secondary
        .iter()
        .all(|offer| !matches!(offer, Offer::PressureTest { .. })));
    assert!(c.state.plans_by_state.is_empty());
    assert_eq!(c.state.plans_unlisted, 0);
    assert_no_plan_write_offers(c);
    assert!(c
        .items
        .iter()
        .filter(|item| item.step == Step::Plans)
        .all(|item| item.status == ItemStatus::Present));
}

fn assert_no_plan_write_offers(checklist: &super::Checklist) {
    let offers = checklist.primary.iter().chain(&checklist.secondary).chain(
        checklist
            .items
            .iter()
            .filter_map(|item| item.offer.as_ref()),
    );
    assert!(offers
        .into_iter()
        .all(|offer| !matches!(offer, Offer::WritePlan { .. })));
}

#[test]
fn only_absent_slugs_get_write_offers_when_a_foreign_entry_is_present() {
    let files = spec(vec![
        owned(metaplan(&["foreign", "absent"])),
        plan("doc/plans/other/PLAN-foreign.md", false),
    ]);
    let (_temp, project) = load(&files);
    let c = checklist(&project);

    assert_eq!(c.current, CurrentStep::Step(Step::Plans));
    assert_eq!(
        c.primary,
        Some(Offer::WritePlan {
            feature: "demo".into(),
            metaplan: "doc/plans/demo/index.md".into(),
            slug: "absent".into(),
        })
    );
    assert!(c
        .secondary
        .iter()
        .all(|offer| !matches!(offer, Offer::WritePlan { .. })));
    let gaps: Vec<_> = c
        .items
        .iter()
        .filter(|item| item.status == ItemStatus::Gap)
        .map(|item| item.path.as_deref())
        .collect();
    assert_eq!(
        gaps,
        [Some(std::path::Path::new("doc/plans/demo/PLAN-absent.md"))]
    );
    assert!(project
        .problems
        .iter()
        .any(|problem| problem.code == "metaplan-entry-foreign-plan"));
}
