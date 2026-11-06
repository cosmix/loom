//! The Built view: which review it reads and that it reads what the generator writes.

use std::path::Path;

use super::{built_view, BuiltView, CandidateKind};
use crate::fs::memory::{append_entry, MemoryEntry, MemoryEntryType};
use crate::fs::plan_review::{
    generate, review_item_line, OPEN_FINDINGS_HEADING, OPEN_QUESTIONS_HEADING,
    SPEC_CONFORMANCE_HEADING, SPEC_CONFORMANCE_PREFIX,
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

/// A ledger entry as the review import writes it, `source` being its `Source:` line.
fn entry(id: &str, source: Option<&str>, status: &str, text: &str) -> String {
    let source = source.map_or(String::new(), |value| format!("Source: {value}\n"));
    format!("## {id} Title\n\nAbout: PLAN-one.md\n{source}Status: {status}\n\n{text}\n\n")
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
}

#[test]
fn a_feature_without_a_review_has_an_empty_view() {
    let temp = tempfile::tempdir().unwrap();
    let view = checklist(temp.path(), &[]);
    assert_eq!(view.built, BuiltView::default());
    assert_eq!(
        built_view(&Project::load(temp.path()).unwrap(), "demo", &[]),
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
    assert!(built.candidates.is_empty(), "no ledger, no candidates");
}

/// The run review the generator writes for a run whose integration-verify stage recorded a
/// decision and a question, both tagged `spec-conformance:`.
fn generated_spec_review() -> String {
    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join(".loom/work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    std::fs::write(work.join("config.toml"), "[plan]\nplan_id = \"PLAN-one\"\n").unwrap();
    let stage = "---\nid: integration-verify\nname: Verify\n---\n";
    std::fs::write(work.join("stages/00-integration-verify.md"), stage).unwrap();
    let tag = SPEC_CONFORMANCE_PREFIX;
    for (kind, text) in [
        (
            MemoryEntryType::Decision,
            format!("{tag} s_01.md: corrected"),
        ),
        (MemoryEntryType::Question, format!("{tag} s_02.md: gap")),
    ] {
        let entry = MemoryEntry::new(kind, text);
        append_entry(&work, "integration-verify", &entry).unwrap();
    }
    let path = generate(&work, scratch.path(), scratch.path(), false).unwrap();
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn a_question_in_both_sections_is_listed_once_as_a_candidate() {
    let temp = tempfile::tempdir().unwrap();
    let text = generated_spec_review();
    // The generator strips the prefix under Spec Conformance only, so the question is in both.
    assert!(text.contains("): spec-conformance: s_02.md: gap"), "{text}");
    let built = checklist(temp.path(), &[("doc/plans/REVIEW-PLAN-one.md", &text)]).built;

    let spec: Vec<_> = built.spec.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(spec, ["s_01.md: corrected"]);
}

/// A Built view over a ledger holding every entry shape: two imported, one by hand, and two that
/// are no longer open.
fn built_from_mixed_ledger(root: &Path) -> BuiltView {
    let review = review("2026-11-01 10:00 UTC", "a finding");
    let imported = |id: &str, section: &str, stage: &str, text: &str| {
        let source = format!("REVIEW-PLAN-one.md#{section} ({stage}, 0123456789ab)");
        entry(id, Some(&source), "open", text)
    };
    let ledger = format!(
        "# Findings\n\n{}{}{}{}{}",
        imported("F-1", "open-findings", "stage", "Imported finding"),
        imported("F-2", "open-questions", "chat", "Imported question"),
        entry("F-3", None, "open", "Found by hand"),
        entry("F-4", None, "planned PLAN-two.md", "Already planned"),
        entry("F-5", None, "dismissed: no", "Dismissed"),
    );
    let files = [
        ("doc/plans/REVIEW-PLAN-one.md", review.as_str()),
        ("doc/plans/demo/findings.md", ledger.as_str()),
    ];
    checklist(root, &files).built
}

#[test]
fn candidates_are_the_open_ledger_entries_in_ledger_order() {
    let temp = tempfile::tempdir().unwrap();
    let built = built_from_mixed_ledger(temp.path());

    let rows: Vec<_> = built
        .candidates
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.kind,
                c.stage.as_str(),
                c.stage_id.as_str(),
                c.review.as_str(),
            )
        })
        .collect();
    let review = "doc/plans/REVIEW-PLAN-one.md";
    assert_eq!(
        rows,
        [
            ("F-1", CandidateKind::Finding, "stage", "stage", review),
            ("F-2", CandidateKind::Question, "chat", "chat", review),
            (
                "F-3",
                CandidateKind::Finding,
                "review",
                "",
                "doc/plans/demo/findings.md"
            ),
        ]
    );
    assert_eq!(
        candidate_texts(&built),
        ["Imported finding", "Imported question", "Found by hand"]
    );
}
