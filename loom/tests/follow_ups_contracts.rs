//! Frozen contracts for feature follow-ups: a finished feature offers only Plan follow-ups,
//! the follow-ups opening appends to the feature's `index.md`, and a feature has one metaplan.

use loom::planning::checklist::{Checklist, CurrentStep, Offer};
use loom::planning::guidance::{opening_message, AuthoringStart};
use loom::planning::kinds::{classify, DocKind};
use loom::planning::project::Project;
use std::fs;
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, content: &str) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

/// A feature with one section and its only plan archived done.
fn done_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "doc/product/demo/index.md",
        "# Demo\n\nA demo.\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n| 01 | Alpha |\n",
    );
    put(
        root.path(),
        "doc/product/demo/section_01.md",
        "# Alpha\n\nBody.\n",
    );
    put(
        root.path(),
        "doc/plans/demo/index.md",
        "# Engineering\n\n## 1. Done (`PLAN-fix.md`)\n",
    );
    put(
        root.path(),
        "doc/plans/demo/archive/DONE-PLAN-fix.md",
        "# Finished\n",
    );
    root
}

fn checklist_of(root: &Path) -> Checklist {
    let project = Project::load(root).unwrap();
    project.feature("demo").unwrap().checklist.clone()
}

fn ids(offers: &[Offer]) -> Vec<String> {
    offers.iter().map(Offer::id).collect()
}

#[test]
fn finished_feature_offers_only_plan_follow_ups() {
    let done = done_fixture();
    let checklist = checklist_of(done.path());
    assert_eq!(checklist.current, CurrentStep::ReadyAllDone);
    assert_eq!(checklist.primary, None);
    // Review the feature is offered after any plan has run; it is not a follow-up offer.
    assert_eq!(
        ids(&checklist.secondary),
        ["plan-follow-ups", "review-feature"]
    );
    assert_eq!(checklist.secondary[0].label(), "Plan follow-ups");

    let ready = done_fixture();
    fs::remove_file(ready.path().join("doc/plans/demo/archive/DONE-PLAN-fix.md")).unwrap();
    put(
        ready.path(),
        "doc/plans/demo/PLAN-fix.md",
        "# Fix\n\nImplements: section_01\n\n## Pressure tests\n\n- 2026-10-26: Claude opus; 1 round\n",
    );
    let checklist = checklist_of(ready.path());
    assert_eq!(checklist.current, CurrentStep::Ready);
    let primary = checklist.primary.as_ref().map(Offer::id);
    assert_eq!(primary.as_deref(), Some("start-run"));
    let offered: Vec<String> = primary
        .into_iter()
        .chain(ids(&checklist.secondary))
        .collect();
    for banned in [concat!("refine-", "spec"), "plan-follow-ups"] {
        assert!(
            !offered.iter().any(|id| id == banned),
            "a ready feature offers {banned}: {offered:?}"
        );
    }
}

#[test]
fn plan_follow_ups_opening_appends_to_the_index() {
    let done = done_fixture();
    let project = Project::load(done.path()).unwrap();
    let start = AuthoringStart::Offer(Offer::PlanFollowUps);
    let opening = opening_message(&start, &project, Some("demo")).unwrap();
    let head: Vec<&str> = opening.lines().take(3).collect();
    assert_eq!(
        head.join("\n"),
        "loom plan: done, Plan follow-ups.\n\
         Append follow-up entries under \"## Follow-ups\" in doc/plans/demo/index.md.\n\
         Load the skill loom-plan:metaplan, then begin by telling me what you read and what you plan to ask.",
        "opening:\n{opening}"
    );
}

#[test]
fn a_second_metaplan_file_is_another_document() {
    let done = done_fixture();
    let second = "doc/plans/demo/metaplan-02.md";
    let misplaced = "doc/plans/optim-metaplan.md";
    put(
        done.path(),
        second,
        "# Round two\n\n## 1. Extra (`PLAN-extra.md`)\n",
    );
    put(done.path(), misplaced, "# Misplaced\n");

    for path in [second, misplaced] {
        assert_eq!(classify(Path::new(path)), DocKind::Other, "{path}");
    }
    let project = Project::load(done.path()).unwrap();
    let feature = project.feature("demo").unwrap();
    let metaplans: Vec<&PathBuf> = feature.metaplans.iter().map(|(path, _)| path).collect();
    assert_eq!(metaplans, [&PathBuf::from("doc/plans/demo/index.md")]);

    let checklist = &feature.checklist;
    let offered: Vec<String> = checklist
        .items
        .iter()
        .filter_map(|item| item.offer.as_ref().map(Offer::id))
        .chain(checklist.primary.as_ref().map(Offer::id))
        .chain(ids(&checklist.secondary))
        .collect();
    assert!(
        !offered.iter().any(|id| id.contains("extra")),
        "the second metaplan adds offers: {offered:?}"
    );
    let moved: Vec<String> = project
        .moves
        .iter()
        .filter(|offer| offer.target().as_deref() == Some(Path::new(misplaced)))
        .map(Offer::id)
        .collect();
    assert!(moved.is_empty(), "offers to move {misplaced}: {moved:?}");
}
