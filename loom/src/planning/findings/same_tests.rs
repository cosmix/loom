use super::*;
use crate::planning::findings::FindingStatus;

fn finding(about: &[&str], evidence: Option<&str>, text: &str) -> Finding {
    Finding {
        id: "F-1".into(),
        title: crate::planning::findings::title(text),
        about: about.iter().map(|token| (*token).to_owned()).collect(),
        evidence: evidence.map(str::to_owned),
        severity: None,
        source: None,
        status: FindingStatus::Open,
        text: text.into(),
    }
}

const STAGE: &[&str] = &["PLAN-trace.md#stage:server"];

// Two run-review items of one plan that reviewers raised twice, in different words and at
// different lines (taken from the planning-trace run review).
#[test]
fn rewordings_of_one_review_item_are_the_same_finding() {
    let pairs = [
        (
            "loom/src/planning/checklist/offers.rs:114 ReviewFeature.target() is None (wire target \
             null) while common.md:323 gives target doc/plans/<f>/findings.md; set it",
            "loom/src/planning/checklist/offers.rs:103 Offer::ReviewFeature has a null wire target \
             while common.md 'Offers and sessions' names target doc/plans/<f>/findings.md; \
             harmless today",
        ),
        (
            "loom/src/plan_server/trace/settle_route.rs:97 The stale check covers only the plan \
             file; a brief edited inside the watcher lag still gives an out-of-date bundle",
            "loom/src/plan_server/trace/settle_route.rs:117 The stale check covers only the \
             document's own file; a plan whose brief bundle or upstream section changed before \
             the watcher batch gets a line built from the old text",
        ),
    ];
    for (a, b) in pairs {
        assert!(
            same_finding(&finding(STAGE, None, a), &finding(&[], None, b)),
            "{a}"
        );
    }
}

#[test]
fn different_claims_about_one_file_stay_apart() {
    let a = "web/src/plan/trace/findings-page.tsx:173 With no follow-up offer the NO_OFFER reason \
             renders twice on every open card; draw it once.";
    let b = "web/src/plan/trace/findings-page.tsx:125 Confirm dismissal shows no busy state while \
             the POST is in flight; pass a busy reason.";
    assert!(!same_finding(
        &finding(STAGE, None, a),
        &finding(STAGE, None, b)
    ));
}

#[test]
fn one_claim_about_two_files_stays_apart() {
    let a = "web/src/a.ts:1 The reason renders twice on every open card; draw it once.";
    let b = "web/src/b.ts:1 The reason renders twice on every open card; draw it once.";
    assert!(!same_finding(
        &finding(STAGE, None, a),
        &finding(STAGE, None, b)
    ));
}

#[test]
fn one_claim_cited_at_distant_lines_of_one_file_stays_apart() {
    let a = "src/a.rs:10 missing test for the retry path in the scheduler";
    let far = "src/a.rs:200 missing test for the timeout path in the scheduler";
    assert!(!same_finding(
        &finding(STAGE, None, a),
        &finding(STAGE, None, far)
    ));
    // Nearby, the two claims still differ in the one word that matters.
    let near = "src/a.rs:12 missing test for the timeout path in the scheduler";
    assert!(!same_finding(
        &finding(STAGE, None, a),
        &finding(STAGE, None, near)
    ));
}

// Imported entries repeat the plan's `Implements:` tokens after their stage; those tokens do not
// place two items of different stages together.
#[test]
fn imported_items_of_different_stages_stay_apart() {
    let text = "The settle route reads the plan file twice and builds the line from the older read";
    let one = &[
        "PLAN-x.md#stage:server",
        "section_04#rebuild-budget",
        "section_05",
    ];
    let other = &[
        "PLAN-x.md#stage:web",
        "section_04#rebuild-budget",
        "section_05",
    ];
    assert!(!same_finding(
        &finding(one, None, text),
        &finding(other, None, text)
    ));
    assert!(same_finding(
        &finding(one, None, text),
        &finding(one, None, text)
    ));
}

#[test]
fn the_same_evidence_merges_only_similar_claims() {
    let quote = "The **rebuild** budget counts plan summaries.";
    let a = finding(
        &["vision", "section_04#rebuild-budget"],
        Some(quote),
        "The rebuild budget ignores brief changes when it counts work.",
    );
    let b = finding(
        &["section_04#rebuild-budget"],
        Some("\"the rebuild  budget counts plan summaries.\""),
        "Rebuild budget counting ignores brief changes entirely.",
    );
    assert!(same_finding(&a, &b), "the quote places them together");
    let other_problem = finding(
        &["section_04#rebuild-budget"],
        b.evidence.as_deref(),
        "Plan summaries are counted twice when a stage is renamed.",
    );
    assert!(!same_finding(&a, &other_problem));
}

#[test]
fn hand_written_claims_compare_by_shared_nodes() {
    let about = &["section_04#rebuild-budget"];
    let a = finding(
        about,
        None,
        "The rebuild budget ignores brief changes when it counts work.",
    );
    let b = finding(
        about,
        None,
        "Rebuild budget counting ignores brief changes entirely.",
    );
    assert!(same_finding(&a, &b));
    assert!(!same_finding(&a, &finding(&["section_09"], None, &b.text)));
}

#[test]
fn quotes_normalize_marks_quotes_case_and_spaces() {
    assert_eq!(
        normalized_quote("  “The `loom plan`\n  *check*  Runs.” "),
        "the loom plan check runs."
    );
}
