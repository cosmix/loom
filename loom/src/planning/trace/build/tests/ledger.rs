//! Findings on headings, their checks, revisit marks and the trace's problems.

use super::{doc, heading, node, section, vision, Fixture, LEDGER, METAPLAN, PLAN_FIRST, VISION};
use crate::planning::trace::{FindingCheck, Gap, GapKind, SettleState};

/// Entry 1's plan is written, entry 2's is not, entry 3's is running.
const ENTRIES: &str = "# E\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n\n\
     ## 2. Second (`PLAN-second.md`)\n\nCovers: section_02\n\n\
     ## 3. Third (`PLAN-third.md`)\n\nCovers: section_01#two\n";

const SECTION_1: &str = "# S1\n\n## One\n\nThe first heading says one thing.\n\n\
     ## Two\n\nThe second heading says two things.\n";
const SECTION_2: &str = "# S2\n\n## Three\n\nThe third heading says *three* things.\n\n\
     ## Four\n\nThe fourth heading says four things.\n";

/// An entry without evidence: what an import writes.
fn entry(id: &str, title: &str, about: &str, status: &str) -> String {
    format!("## {id} {title}\n\nAbout: {about}\nStatus: {status}\n\nText.\n\n")
}

/// An entry with an `Evidence:` quote and a severity: what a review writes.
fn verified(id: &str, about: &str, evidence: &str, status: &str) -> String {
    claim(id, about, evidence, status, &format!("Text of {id}."))
}

/// A verified entry with its own text, for a repeat of an earlier claim.
fn claim(id: &str, about: &str, evidence: &str, status: &str, text: &str) -> String {
    format!(
        "## {id} Title {id}\n\nAbout: {about}\nEvidence: {evidence}\nSeverity: medium\n\
         Status: {status}\n\n{text}\n\n"
    )
}

/// The claim the repeated entries make.
const REPEATED: &str = "The heading names fewer things than the vision promises the reader.";

fn ledger() -> String {
    let entries = [
        verified("F-1", "section_01#two", "second heading says two", "open"),
        claim(
            "F-2",
            "section_02#three",
            "the third heading says three things.",
            "open",
            REPEATED,
        ),
        verified("F-3", "section_02#four", "heading says four things", "open"),
        entry("F-4", "Unverified", "section_02#three", "open"),
        verified("F-5", "section_02#four", "heading says five things", "open"),
        verified(
            "F-6",
            "section_02#three",
            "third heading says",
            "dismissed: not a problem",
        ),
        verified(
            "F-7",
            "section_01",
            "first heading says one",
            "addressed section_01#one",
        ),
        claim(
            "F-8",
            "section_02#three",
            "The third heading says three things.",
            "open",
            REPEATED,
        ),
    ];
    format!("# Findings\n\n{}", entries.concat())
}

fn revisit_fixture() -> Fixture {
    Fixture::new(&[
        (VISION, &vision(&[1, 2])),
        (&section(1), SECTION_1),
        (&section(2), SECTION_2),
        (METAPLAN, ENTRIES),
        (PLAN_FIRST, "# First\n\nImplements: section_02#four\n"),
        (
            "doc/plans/demo/IN_PROGRESS-PLAN-third.md",
            "# Third\n\nImplements: section_01#two\n",
        ),
        (LEDGER, &ledger()),
    ])
}

#[test]
fn findings_attach_to_the_headings_their_about_names() {
    let trace = revisit_fixture().trace();
    let ids = |rel: &str, anchor: &str| heading(&trace, rel, anchor).findings.clone();
    assert_eq!(ids(&section(1), "one"), ["F-7"]);
    assert_eq!(ids(&section(1), "two"), ["F-1", "F-7"]);
    assert_eq!(ids(&section(2), "three"), ["F-2", "F-4", "F-6", "F-8"]);
    assert_eq!(ids(&section(2), "four"), ["F-3", "F-5"]);
    assert_eq!(trace.findings[6].about, [node(&section(1), None)]);
    assert_eq!(trace.findings[6].changed, [node(&section(1), Some("one"))]);
}

#[test]
fn checks_verify_quotes_in_their_passage_and_mark_repeats() {
    let fx = revisit_fixture();
    let project = fx.load();
    let trace = &project.feature("demo").unwrap().trace;
    let checks: Vec<&FindingCheck> = trace.findings.iter().map(|f| &f.check).collect();
    let dup = FindingCheck::Duplicate("F-2".into());
    use FindingCheck::{Stale, Unverified, Verified};
    let expected = [
        &Verified,
        &Verified,
        &Verified,
        &Unverified,
        &Stale,
        &Verified,
        &Verified,
        &dup,
    ];
    assert_eq!(checks, expected);
    let missing: Vec<Option<usize>> = project
        .problems
        .iter()
        .filter(|p| p.code == "findings-evidence-missing")
        .map(|p| p.line)
        .collect();
    assert_eq!(missing.len(), 1, "only the open stale F-5: {missing:?}");
    // A quote from another heading of the same section is not evidence for this one.
    fx.put(&section(2), &SECTION_2.replace("three* things", "3* items"));
    let trace = fx.trace();
    assert_eq!(trace.findings[1].check, Stale);
    assert_eq!(trace.findings[7].check, Stale, "F-2 stale is no original");
}

#[test]
fn revisit_names_actionable_findings_until_the_plan_runs() {
    let fx = revisit_fixture();
    let trace = fx.trace();
    let revisit: Vec<Vec<String>> = trace.entries.iter().map(|e| e.revisit.clone()).collect();
    assert_eq!(revisit, [vec!["F-3"], vec!["F-2", "F-3"], vec![]]);
    let revisits: Vec<&Gap> = trace
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::Revisit)
        .collect();
    let second = node(METAPLAN, Some(trace.entries[1].anchor.as_str()));
    assert_eq!(revisits[0].node, node(PLAN_FIRST, None));
    assert_eq!(revisits[0].count, Some(1));
    assert_eq!(revisits[1].node, second);
    assert_eq!(revisits[1].count, Some(2));
    assert_eq!(revisits.len(), 2);
    let triage = trace.gaps.last().unwrap();
    assert_eq!((triage.kind, triage.count), (GapKind::Triage, Some(3)));
    assert_eq!(triage.node, node(LEDGER, None));

    fx.rename(PLAN_FIRST, "doc/plans/demo/IN_PROGRESS-PLAN-first.md");
    let trace = fx.trace();
    assert!(trace.entries[0].revisit.is_empty());
    assert_eq!(trace.entries[1].revisit, ["F-2", "F-3"]);
}

/// The checks of a ledger over section 1 holding `entries`.
fn checks_of(entries: &[String]) -> Vec<FindingCheck> {
    let ledger = format!("# Findings\n\n{}", entries.concat());
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), SECTION_1),
        (LEDGER, &ledger),
    ]);
    fx.trace().findings.into_iter().map(|f| f.check).collect()
}

#[test]
fn a_dismissed_entry_is_no_original_and_an_addressed_one_is() {
    let quote = "first heading says one";
    let dismissed = "dismissed: recorded again as F-2";
    let checks = checks_of(&[
        claim("F-1", "section_01#one", quote, dismissed, REPEATED),
        claim("F-2", "section_01#one", quote, "open", REPEATED),
        claim(
            "F-3",
            "section_01#two",
            "second heading says",
            "addressed section_01#two",
            REPEATED,
        ),
        claim(
            "F-4",
            "section_01#two",
            "second heading says",
            "open",
            REPEATED,
        ),
    ]);
    use FindingCheck::{Duplicate, Verified};
    assert_eq!(
        checks,
        [Verified, Verified, Verified, Duplicate("F-3".into())]
    );
}

// Regression: an unaddressed entry counted as an original, so a re-raise of a finding the team
// was stopped on read as a duplicate and the finding could never return.
#[test]
fn an_unaddressed_entry_is_no_original() {
    let quote = "first heading says one";
    let stopped = "unaddressed: the team was stopped";
    let checks = checks_of(&[
        claim("F-1", "section_01#one", quote, stopped, REPEATED),
        claim("F-2", "section_01#one", quote, "open", REPEATED),
    ]);
    assert_eq!(checks, [FindingCheck::Verified, FindingCheck::Verified]);
}

// Regression: an addressed entry absorbed every later report of the same issue, so a review that
// re-recorded a regression with new evidence saw it marked a duplicate.
#[test]
fn a_regression_of_an_addressed_finding_with_new_evidence_stands() {
    let addressed = "addressed section_01#two";
    let checks = checks_of(&[
        claim(
            "F-1",
            "section_01#two",
            "says two things",
            addressed,
            REPEATED,
        ),
        claim(
            "F-2",
            "section_01#two",
            "second heading says two",
            "open",
            REPEATED,
        ),
        // The addressed quote is gone from its passage; the new one is there.
        claim(
            "F-3",
            "section_01#one",
            "heading says nothing",
            addressed,
            REPEATED,
        ),
        claim(
            "F-4",
            "section_01#one",
            "heading says one thing",
            "open",
            REPEATED,
        ),
        // The same quote again, or no quote: a re-raise, not a regression.
        claim("F-5", "section_01#two", "says two things", "open", REPEATED),
        entry("F-6", "Title F-6", "section_01#two", "open").replace("Text.", REPEATED),
    ]);
    use FindingCheck::{Duplicate, Stale, Verified};
    let dup = Duplicate("F-1".into());
    assert_eq!(
        checks,
        [Verified, Verified, Stale, Verified, dup.clone(), dup]
    );
}

/// A section with a malformed and a second `Settled:` line, a locked plan with a malformed one,
/// and a ledger with a bad heading and an entry with four unresolved `About:` tokens.
fn problem_fixture() -> Fixture {
    let ledger = "# Findings\n\n## Notes\n\nFree text.\n\n## F-1 Bad tokens\n\n\
         About: section_09#x, bogus, section_01#missing, section_01#one, PLAN-nowhere.md\n\n\
         Text.\n";
    let section_text = "# S1\n\nSettled: bad\n\nSettled: 0123456789ab\n\n## One\n\nText.\n";
    Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), section_text),
        (
            "doc/plans/demo/IN_PROGRESS-PLAN-x.md",
            "# X\n\nSettled: bad\n\nImplements: none\n",
        ),
        (LEDGER, ledger),
    ])
}

#[test]
fn problems_cover_settled_lines_about_tokens_and_the_ledger() {
    let project = problem_fixture().load();
    let codes = [
        "settled-bad",
        "settled-duplicate",
        "about-unresolved",
        "findings-bad-entry",
        "findings-bad-status",
    ];
    let found: Vec<(String, Option<usize>, &str)> = project
        .problems
        .iter()
        .filter(|p| codes.contains(&p.code))
        .map(|p| (p.path.display().to_string(), p.line, p.code))
        .collect();
    let (ledger, s1) = (LEDGER.to_owned(), section(1));
    let expected = [
        (ledger.clone(), Some(3), "findings-bad-entry"),
        (ledger.clone(), Some(7), "about-unresolved"),
        (ledger.clone(), Some(7), "about-unresolved"),
        (ledger.clone(), Some(7), "about-unresolved"),
        (ledger.clone(), Some(7), "about-unresolved"),
        (ledger, Some(7), "findings-bad-status"),
        (s1.clone(), Some(3), "settled-bad"),
        (s1, Some(5), "settled-duplicate"),
    ];
    assert_eq!(found, expected);
    let trace = &project.feature("demo").unwrap().trace;
    assert_eq!(trace.findings[0].about, [node(&section(1), Some("one"))]);
    assert_eq!(doc(trace, &section(1)).settle, SettleState::Revised);
}

#[test]
fn an_addressed_status_naming_nothing_is_a_problem() {
    let ledger = format!(
        "# Findings\n\n{}",
        entry(
            "F-1",
            "Gone",
            "vision",
            "addressed section_01#nope, section_01"
        )
    );
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), SECTION_1),
        (LEDGER, &ledger),
    ]);
    let project = fx.load();
    let found: Vec<&str> = project
        .problems
        .iter()
        .filter(|p| p.code == "about-unresolved")
        .map(|p| p.message.as_str())
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("Status: addressed token `section_01#nope`"));
    let trace = &project.feature("demo").unwrap().trace;
    assert_eq!(trace.findings[0].changed, [node(&section(1), None)]);
}
