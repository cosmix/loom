//! The Built view: which review it reads and that it reads what the generator writes.

use std::path::Path;

use super::{built_view, BuiltView, CandidateKind};
use crate::fs::plan_review::{
    review_item_line, OPEN_FINDINGS_HEADING, OPEN_QUESTIONS_HEADING, SPEC_CONFORMANCE_HEADING,
};
use crate::plan_server::wire::checklist::{ChecklistView, ProjectChecklistView};
use crate::planning::project::Project;

const METAPLAN: &str = "# Demo\n\n## 1. One (`PLAN-one.md`)\n";
const PLAN: (&str, &str) = ("doc/plans/demo/archive/DONE-PLAN-one.md", "# Plan: one\n");

fn review(stamp: &str, finding: &str) -> String {
    format!(
        "# Code Review: one\n\n**Plan:** PLAN-one | **Generated:** {stamp}\n\n{SPEC_CONFORMANCE_HEADING}\n\n\
         No spec conformance outcomes.\n\n{OPEN_QUESTIONS_HEADING}\n\nNo open questions.\n\n\
         {OPEN_FINDINGS_HEADING}\n\n{}\n",
        review_item_line("Stage", "stage", finding)
    )
}

fn checklist(root: &Path, files: &[(&str, &str)]) -> ChecklistView {
    let all = [("doc/plans/demo/index.md", METAPLAN), PLAN];
    for (path, text) in all.iter().chain(files) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let project = Project::load(root).unwrap();
    let mut checklists = ProjectChecklistView::from(&project).checklists;
    checklists.remove(0)
}

fn candidate_texts(built: &BuiltView) -> Vec<&str> {
    built.candidates.iter().map(|c| c.text.as_str()).collect()
}

#[test]
fn the_newer_of_two_reviews_wins_by_its_generated_stamp() {
    let temp = tempfile::tempdir().unwrap();
    let older = review("2026-11-01 10:00 UTC", "older finding");
    let newer = review("2026-11-02 10:00 UTC", "newer finding");
    // The newer stamp sits on the path that sorts first, so a last-path choice would be wrong.
    let built = checklist(
        temp.path(),
        &[
            ("doc/plans/demo/archive/REVIEW-DONE-PLAN-one.md", &newer),
            ("doc/plans/demo/archive/REVIEW-PLAN-one.md", &older),
        ],
    )
    .built;

    assert_eq!(built.reviews.len(), 1);
    assert_eq!(
        built.reviews[0].path,
        "doc/plans/demo/archive/REVIEW-DONE-PLAN-one.md"
    );
    assert_eq!(candidate_texts(&built), ["newer finding"]);
}

#[test]
fn a_feature_without_a_review_has_an_empty_view() {
    let temp = tempfile::tempdir().unwrap();
    let view = checklist(temp.path(), &[]);
    assert_eq!(view.built, BuiltView::default());
    assert_eq!(
        built_view(&Project::load(temp.path()).unwrap(), &[]),
        view.built
    );
}

#[test]
fn a_review_in_the_plan_home_and_one_at_the_top_are_both_found() {
    let text = review("2026-11-01 10:00 UTC", "a finding");
    for path in [
        "doc/plans/demo/archive/REVIEW-PLAN-one.md",
        "doc/plans/REVIEW-PLAN-one.md",
    ] {
        let alone = tempfile::tempdir().unwrap();
        let built = checklist(alone.path(), &[(path, &text)]).built;
        assert_eq!(built.reviews[0].path, path);
    }
    // Both on disk at once: whichever carries the newer stamp is the one entry, so each location
    // is read (a location that was skipped could never win).
    const HOME: &str = "doc/plans/demo/archive/REVIEW-PLAN-one.md";
    const TOP: &str = "doc/plans/REVIEW-PLAN-one.md";
    let older = review("2026-11-01 10:00 UTC", "older finding");
    let newer = review("2026-11-02 10:00 UTC", "newer finding");
    for (home_text, top_text, winner) in [(&newer, &older, HOME), (&older, &newer, TOP)] {
        let both = tempfile::tempdir().unwrap();
        let built = checklist(both.path(), &[(HOME, home_text), (TOP, top_text)]).built;
        assert_eq!(built.reviews.len(), 1);
        assert_eq!(built.reviews[0].path, winner);
        assert_eq!(candidate_texts(&built), ["newer finding"]);
    }
}

#[test]
fn the_wire_reads_what_the_generator_writes() {
    let temp = tempfile::tempdir().unwrap();
    let text = format!(
        "# Code Review: one\n\n**Plan:** PLAN-one | **Generated:** 2026-11-01 10:00 UTC\n\n\
         {SPEC_CONFORMANCE_HEADING}\n\n{}\n\n{OPEN_QUESTIONS_HEADING}\n\n{}\n\n\
         {OPEN_FINDINGS_HEADING}\n\n{}\n",
        review_item_line(
            "Verify *it*",
            "integration-verify",
            "Fixed the spec\nto match"
        ),
        review_item_line("Chat", "chat", "Close on send?"),
        review_item_line("Polish", "polish", "Bland stats"),
    );
    let path = "doc/plans/REVIEW-PLAN-one.md";
    let built = checklist(temp.path(), &[(path, &text)]).built;

    assert_eq!(built.spec.len(), 1);
    let spec = &built.spec[0];
    assert_eq!(
        (
            spec.stage.as_str(),
            spec.stage_id.as_str(),
            spec.text.as_str()
        ),
        (
            "Verify *it*",
            "integration-verify",
            "Fixed the spec to match"
        )
    );
    assert_eq!(spec.review, path);
    let kinds: Vec<_> = built
        .candidates
        .iter()
        .map(|c| (c.kind, c.text.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (CandidateKind::Finding, "Bland stats"),
            (CandidateKind::Question, "Close on send?")
        ]
    );
}
