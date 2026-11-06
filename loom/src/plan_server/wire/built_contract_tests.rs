//! Contract: the Built view reads the newest run review of each done plan.

use std::fs;
use std::path::Path;

use crate::plan_server::wire::checklist::{
    BuiltView, CandidateKind, CandidateView, ChecklistView, ProjectChecklistView, RunReviewView,
    SpecItemView,
};
use crate::planning::project::Project;

const VISION: &str = "# Alpha\n\nA demo.\n\n## Specification sections\n\n\
    | # | Section |\n| --- | --- |\n| 01 | Export |\n";

const METAPLAN: &str = "# Alpha engineering\n\n## 1. One (`PLAN-alpha-01.md`)\n\n\
    ## 2. Two (`PLAN-alpha-02.md`)\n\n## 3. Three (`PLAN-alpha-03.md`)\n";

const REVIEW_01: &str = "# Code Review: Alpha one\n\n\
**Plan:** PLAN-alpha-01 | **Generated:** 2026-11-01 10:00 UTC\n\n\
## Summary\n\nBuilt alpha one.\n\n\
## Changes by Stage\n\n\
### Integration Verification (integration-verify)\n\n\
**Status:** Completed\n**Purpose:** Verify the plan.\n\n\
#### Files Changed\n\nNo changes recorded.\n\n\
#### Key Decisions\n\n- Rebuilt the web bundle\n\n\
#### Notes\n\nNo notes recorded.\n\n\
## Spec Conformance\n\n\
- **Integration Verification** (`integration-verify`): Corrected doc/product/alpha/section_01.md to match the build: exports are CSV\n\n\
## Open Questions\n\n\
- **Integration Verification** (`integration-verify`): Does section_01 cover empty exports?\n\
- **Chat polish** (`chat-polish`): Should the sheet close on send?\n\n\
## Open Findings\n\n\
- **Chat polish** (`chat-polish`): Settings statistics look bland\n";

const REVIEW_02: &str = "# Code Review: Alpha two\n\n\
**Plan:** PLAN-alpha-02 | **Generated:** 2026-11-02 10:00 UTC\n\n\
## Summary\n\nBuilt alpha two.\n\n\
## Changes by Stage\n\nNo stage memory recorded.\n\n\
## Open Questions\n\nNo open questions.\n\n\
## Open Findings\n\nNo open findings.\n";

/// The ledger the run-review import writes for the open items of `REVIEW_01`, plus an entry that
/// is no longer open.
fn findings_ledger() -> String {
    let entry = |n: u8, section: &str, stage: &str, text: &str, status: &str| {
        format!(
            "## F-{n} {text}\n\nAbout: PLAN-alpha-01.md#stage:{stage}\n\
             Source: REVIEW-PLAN-alpha-01.md#{section} ({stage}, 0123456789ab)\n\
             Status: {status}\n\n{text}\n\n"
        )
    };
    let open = "open";
    [
        "# Findings\n\n".to_owned(),
        entry(
            1,
            "open-questions",
            "integration-verify",
            "Does section_01 cover empty exports?",
            open,
        ),
        entry(
            2,
            "open-questions",
            "chat-polish",
            "Should the sheet close on send?",
            open,
        ),
        entry(
            3,
            "open-findings",
            "chat-polish",
            "Settings statistics look bland",
            open,
        ),
        entry(
            4,
            "open-findings",
            "chat-polish",
            "Not done yet",
            "dismissed: later",
        ),
    ]
    .concat()
}

const REVIEW_01_PATH: &str = "doc/plans/REVIEW-PLAN-alpha-01.md";
const REVIEW_02_PATH: &str = "doc/plans/alpha/archive/REVIEW-PLAN-alpha-02.md";

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn alpha_checklist(root: &Path) -> ChecklistView {
    let review_03 = REVIEW_01
        .replace("alpha one", "alpha three")
        .replace("PLAN-alpha-01", "PLAN-alpha-03")
        .replace("Settings statistics look bland", "Not done yet");
    let findings = findings_ledger();
    for (path, text) in [
        ("doc/product/alpha/index.md", VISION),
        ("doc/product/alpha/section_01.md", "# Export\n\nBody.\n"),
        ("doc/plans/alpha/index.md", METAPLAN),
        (
            "doc/plans/alpha/archive/DONE-PLAN-alpha-01.md",
            "# Plan: alpha-01\n",
        ),
        (
            "doc/plans/alpha/archive/DONE-PLAN-alpha-02.md",
            "# Plan: alpha-02\n",
        ),
        ("doc/plans/alpha/PLAN-alpha-03.md", "# Plan: alpha-03\n"),
        ("doc/plans/alpha/findings.md", findings.as_str()),
        (REVIEW_01_PATH, REVIEW_01),
        (REVIEW_02_PATH, REVIEW_02),
        ("doc/plans/REVIEW-PLAN-alpha-03.md", review_03.as_str()),
    ] {
        write(root, path, text);
    }
    let project = Project::load(root).unwrap();
    ProjectChecklistView::from(&project)
        .checklists
        .into_iter()
        .find(|checklist| checklist.feature == "alpha")
        .expect("alpha checklist")
}

fn review_rows(built: &BuiltView) -> Vec<(&str, &str)> {
    let rows = built.reviews.iter();
    rows.map(|RunReviewView { slug, path }| (slug.as_str(), path.as_str()))
        .collect()
}

fn spec_rows(built: &BuiltView) -> Vec<[&str; 4]> {
    let rows = built.spec.iter().map(|item| {
        let SpecItemView {
            stage,
            stage_id,
            text,
            review,
        } = item;
        [
            stage.as_str(),
            stage_id.as_str(),
            text.as_str(),
            review.as_str(),
        ]
    });
    rows.collect()
}

fn candidate_row(candidate: &CandidateView) -> (&str, &str, &str, &str, &str) {
    let CandidateView {
        id: _,
        kind,
        stage,
        stage_id,
        text,
        review,
    } = candidate;
    let kind = if matches!(kind, CandidateKind::Finding) {
        "finding"
    } else if matches!(kind, CandidateKind::Question) {
        "question"
    } else {
        "other"
    };
    (
        kind,
        stage.as_str(),
        stage_id.as_str(),
        text.as_str(),
        review.as_str(),
    )
}

#[test]
fn built_view_reads_the_newest_run_review_of_each_done_plan() {
    let temp = tempfile::tempdir().unwrap();
    let built = alpha_checklist(temp.path()).built;

    assert_eq!(
        review_rows(&built),
        [("alpha-01", REVIEW_01_PATH), ("alpha-02", REVIEW_02_PATH)]
    );
    assert_eq!(
        spec_rows(&built),
        [[
            "Integration Verification",
            "integration-verify",
            "Corrected doc/product/alpha/section_01.md to match the build: exports are CSV",
            REVIEW_01_PATH,
        ]]
    );

    let candidates: Vec<_> = built.candidates.iter().map(candidate_row).collect();
    assert_eq!(candidates.len(), 3, "{candidates:?}");
    let finding = (
        "finding",
        "chat-polish",
        "chat-polish",
        "Settings statistics look bland",
        REVIEW_01_PATH,
    );
    assert!(candidates.contains(&finding), "{candidates:?}");
    for (stage_id, text) in [
        ("chat-polish", "Should the sheet close on send?"),
        ("integration-verify", "Does section_01 cover empty exports?"),
    ] {
        assert!(
            candidates
                .iter()
                .any(|c| c.0 == "question" && c.2 == stage_id && c.3 == text),
            "missing question {text}: {candidates:?}"
        );
    }
    assert!(!candidates.iter().any(|c| c.3 == "Not done yet"));
}
