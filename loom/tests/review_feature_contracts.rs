//! Contract for the `Review the feature` offer: offered once a plan of the feature has run,
//! and opening an authoring session on the feature's findings ledger with the review skill.

use loom::planning::checklist::Offer;
use loom::planning::guidance::{opening_message, AuthoringStart};
use loom::planning::project::Project;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn put(root: &Path, rel: &str, content: &str) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

/// The DEMO fixture of `planning_trace_contracts.rs`: a vision of two written sections, a
/// metaplan of two entries and one written plan, `PLAN-first.md`, not yet run.
fn demo() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(
        root,
        "doc/product/demo/index.md",
        "# Demo\n\nA demo.\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n\
         | 01 | Alpha |\n| 02 | Beta |\n",
    );
    put(
        root,
        "doc/product/demo/section_01.md",
        "# Alpha\n\n## One\n\nText one.\n\n## Two\n\nText two.\n\n## Open questions\n\nNone.\n",
    );
    put(
        root,
        "doc/product/demo/section_02.md",
        "# Beta\n\n## Three\n\nText three.\n",
    );
    put(
        root,
        "doc/plans/demo/index.md",
        "# Engineering\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n\n\
         First plan.\n\n## 2. Second (`PLAN-second.md`)\n\nCovers: section_02\n\nSecond plan.\n",
    );
    put(
        root,
        "doc/plans/demo/PLAN-first.md",
        "# First\n\nImplements: section_01#one\n\nBody.\n",
    );
    temp
}

/// Every offer of the demo checklist as `(id, label)`, primary first.
fn offers(project: &Project) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let checklist = &project.feature("demo").expect("feature demo").checklist;
    let pair = |offer: &Offer| (offer.id(), offer.label());
    let primary = checklist.primary.iter().map(pair).collect();
    let secondary = checklist.secondary.iter().map(pair).collect();
    (primary, secondary)
}

#[test]
fn review_feature_offered_after_a_run() {
    let fresh = demo();
    let (primary, secondary) = offers(&Project::load(fresh.path()).unwrap());
    assert!(
        primary
            .iter()
            .chain(&secondary)
            .all(|(id, _)| id != "review-feature"),
        "offered before any plan ran: {primary:?} {secondary:?}"
    );

    let ran = demo();
    put(
        ran.path(),
        "doc/plans/demo/archive/DONE-PLAN-second.md",
        "# Second\n",
    );
    let project = Project::load(ran.path()).unwrap();
    let (_, secondary) = offers(&project);
    assert!(
        secondary.contains(&("review-feature".into(), "Review the feature".into())),
        "secondary offers: {secondary:?}"
    );

    let start = AuthoringStart::Offer(Offer::ReviewFeature);
    let message = opening_message(&start, &project, Some("demo")).unwrap();
    let lines: Vec<&str> = message.lines().collect();
    assert_eq!(
        lines.get(1).copied(),
        Some("Record findings from reviewing the feature in doc/plans/demo/findings.md."),
        "opening:\n{message}"
    );
    assert!(
        lines
            .get(2)
            .is_some_and(|line| line.contains("loom-plan:review")),
        "opening:\n{message}"
    );
}
