//! Candidate follow-ups: which ledger entries are listed and which run review each one names.
//! Every entry here is verified (its quote is in the plan), so only status and checks decide.

use std::path::Path;

use super::{ledger_candidates, CandidateKind, CandidateView};
use crate::fs::plan_review::{review_item_line, OPEN_FINDINGS_HEADING, OPEN_QUESTIONS_HEADING};
use crate::plan_server::wire::checklist::ProjectChecklistView;
use crate::planning::project::Project;

const METAPLAN: &str = "# Demo\n\n## 1. One (`PLAN-one.md`)\n";
const PLAN: (&str, &str) = (
    "doc/plans/demo/archive/DONE-PLAN-one.md",
    "# Plan: one\n\nThe plan builds part F-1.\nThe plan builds part F-2.\n\
     The plan builds part F-3.\nThe plan builds part F-4.\nThe plan builds part F-5.\n",
);
const LEDGER: &str = "doc/plans/demo/findings.md";

fn review(stamp: &str, finding: &str) -> String {
    format!(
        "# Code Review: one\n\n**Plan:** PLAN-one | **Generated:** {stamp}\n\n\
         {OPEN_QUESTIONS_HEADING}\n\nNo open questions.\n\n{OPEN_FINDINGS_HEADING}\n\n{}\n",
        review_item_line("Stage", "stage", finding)
    )
}

/// A verified ledger entry, `source` being its `Source:` line; its quote names its own id.
fn entry(id: &str, source: Option<&str>, status: &str, text: &str) -> String {
    let source = source.map_or(String::new(), |value| format!("Source: {value}\n"));
    format!(
        "## {id} Title\n\nAbout: PLAN-one.md\nEvidence: The plan builds part {id}.\n\
         Severity: medium\n{source}Status: {status}\n\n{text}\n\n"
    )
}

/// A ledger entry imported from `REVIEW-PLAN-one.md`, then verified by a review.
fn imported(id: &str, section: &str, stage: &str, text: &str) -> String {
    let source = format!("REVIEW-PLAN-one.md#{section} ({stage}, 0123456789ab)");
    entry(id, Some(&source), "open", text)
}

fn candidates(root: &Path, files: &[(&str, &str)]) -> Vec<CandidateView> {
    let all = [("doc/plans/demo/index.md", METAPLAN), PLAN];
    for (path, text) in all.iter().chain(files) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let project = Project::load(root).unwrap();
    let mut checklists = ProjectChecklistView::from(&project).checklists;
    let view = checklists.remove(0);
    assert_eq!(view.candidates, ledger_candidates(&project, "demo"));
    view.candidates
}

/// The review path the one imported candidate names, with `reviews` on disk beside the ledger.
fn named_review(reviews: &[(&str, &str)]) -> String {
    let temp = tempfile::tempdir().unwrap();
    let ledger = format!(
        "# Findings\n\n{}",
        imported("F-1", "open-findings", "stage", "A finding")
    );
    let mut files = vec![(LEDGER, ledger.as_str())];
    files.extend_from_slice(reviews);
    let found = candidates(temp.path(), &files);
    assert_eq!(found.len(), 1, "{found:?}");
    found[0].review.clone()
}

#[test]
fn a_feature_without_a_ledger_has_no_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let text = review("2026-11-01 10:00 UTC", "a finding");
    let found = candidates(temp.path(), &[("doc/plans/REVIEW-PLAN-one.md", &text)]);
    assert!(found.is_empty(), "no ledger, no candidates: {found:?}");
}

#[test]
fn the_newer_of_two_reviews_wins_by_its_generated_stamp() {
    let older = review("2026-11-01 10:00 UTC", "older finding");
    let newer = review("2026-11-02 10:00 UTC", "newer finding");
    // The newer stamp sits on the path that sorts first, so a last-path choice would be wrong.
    let named = named_review(&[
        ("doc/plans/demo/archive/REVIEW-DONE-PLAN-one.md", &newer),
        ("doc/plans/demo/archive/REVIEW-PLAN-one.md", &older),
    ]);
    assert_eq!(named, "doc/plans/demo/archive/REVIEW-DONE-PLAN-one.md");
}

#[test]
fn a_review_in_the_plan_home_and_one_at_the_top_are_both_found() {
    const HOME: &str = "doc/plans/demo/archive/REVIEW-PLAN-one.md";
    const TOP: &str = "doc/plans/REVIEW-PLAN-one.md";
    let text = review("2026-11-01 10:00 UTC", "a finding");
    for path in [HOME, TOP] {
        assert_eq!(named_review(&[(path, &text)]), path);
    }
    // Both on disk at once: whichever carries the newer stamp is named, so each location is read
    // (a location that was skipped could never win).
    let older = review("2026-11-01 10:00 UTC", "older finding");
    let newer = review("2026-11-02 10:00 UTC", "newer finding");
    for (home_text, top_text, winner) in [(&newer, &older, HOME), (&older, &newer, TOP)] {
        assert_eq!(named_review(&[(HOME, home_text), (TOP, top_text)]), winner);
    }
}

#[test]
fn an_imported_entry_without_its_review_names_the_ledger() {
    assert_eq!(named_review(&[]), LEDGER);
}

type Row<'a> = (&'a str, CandidateKind, &'a str, &'a str, &'a str, &'a str);

fn rows_of(found: &[CandidateView]) -> Vec<Row<'_>> {
    found
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.kind,
                c.stage.as_str(),
                c.stage_id.as_str(),
                c.review.as_str(),
                c.text.as_str(),
            )
        })
        .collect()
}

#[test]
fn candidates_are_the_actionable_ledger_entries_in_ledger_order() {
    let temp = tempfile::tempdir().unwrap();
    let review = review("2026-11-01 10:00 UTC", "a finding");
    let ledger = format!(
        "# Findings\n\n{}{}{}{}{}",
        imported("F-1", "open-findings", "stage", "Imported finding"),
        imported("F-2", "open-questions", "chat", "Imported question"),
        entry("F-3", None, "open", "Found by hand"),
        entry("F-4", None, "addressed PLAN-one.md", "Already addressed"),
        entry("F-5", None, "dismissed: no", "Dismissed"),
    );
    let files = [
        ("doc/plans/REVIEW-PLAN-one.md", review.as_str()),
        (LEDGER, ledger.as_str()),
    ];
    let found = candidates(temp.path(), &files);

    let rows = rows_of(&found);
    let top = "doc/plans/REVIEW-PLAN-one.md";
    assert_eq!(
        rows,
        [
            (
                "F-1",
                CandidateKind::Finding,
                "stage",
                "stage",
                top,
                "Imported finding"
            ),
            (
                "F-2",
                CandidateKind::Question,
                "chat",
                "chat",
                top,
                "Imported question"
            ),
            (
                "F-3",
                CandidateKind::Finding,
                "review",
                "",
                LEDGER,
                "Found by hand"
            ),
        ]
    );
}

#[test]
fn unverified_stale_and_duplicate_entries_are_no_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let unverified = "## F-2 Raw\n\nAbout: PLAN-one.md\nStatus: open\n\nNo quote yet.\n\n";
    let stale = entry("F-3", None, "open", "Quoted wrong").replace("part F-3", "part nine");
    // The same quote and the same claim: a repeat of F-1.
    let claim = "The plan builds the part without the retry budget the spec promises.";
    let ledger = format!(
        "# Findings\n\n{}{unverified}{stale}{}",
        entry("F-1", None, "open", claim),
        entry("F-4", None, "open", claim).replace("part F-4", "part F-1"),
    );
    let found = candidates(temp.path(), &[(LEDGER, ledger.as_str())]);
    let ids: Vec<&str> = found.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["F-1"]);
}
