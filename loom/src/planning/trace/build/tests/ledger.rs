//! Findings on headings, revisit marks and the trace's problems.

use super::{doc, heading, node, section, vision, Fixture, LEDGER, METAPLAN, PLAN_FIRST, VISION};
use crate::planning::trace::{Gap, GapKind, SettleState};

/// Entry 1's plan is written, entry 2's is not, entry 3's is running.
const ENTRIES: &str = "# E\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n\n\
     ## 2. Second (`PLAN-second.md`)\n\nCovers: section_02\n\n\
     ## 3. Third (`PLAN-third.md`)\n\nCovers: section_01#two\n";

fn entry(id: &str, title: &str, about: &str, status: &str) -> String {
    format!("## {id} {title}\n\nAbout: {about}\nStatus: {status}\n\nText.\n\n")
}

fn ledger() -> String {
    let entries = [
        entry("F-1", "Two", "section_01#two", "open"),
        entry("F-2", "Three", "section_02#three", "open"),
        entry("F-3", "Four", "section_02#four", "open"),
        entry(
            "F-4",
            "Folded",
            "vision",
            "folded metaplan#2-second-plan-secondmd",
        ),
        entry(
            "F-5",
            "Folded late",
            "vision",
            "folded metaplan#3-third-plan-thirdmd",
        ),
        entry(
            "F-6",
            "Dismissed",
            "section_02#three",
            "dismissed: not a problem",
        ),
        entry("F-7", "Planned", "section_01", "planned PLAN-first.md"),
    ];
    format!("# Findings\n\n{}", entries.concat())
}

fn revisit_fixture() -> Fixture {
    Fixture::new(&[
        (VISION, &vision(&[1, 2])),
        (&section(1), "# S1\n\n## One\n\nText.\n\n## Two\n\nText.\n"),
        (
            &section(2),
            "# S2\n\n## Three\n\nText.\n\n## Four\n\nText.\n",
        ),
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
    assert_eq!(ids(&section(2), "three"), ["F-2", "F-6"]);
    assert_eq!(ids(&section(2), "four"), ["F-3"]);
    assert_eq!(trace.findings[3].about, [node(VISION, None)]);
}

#[test]
fn revisit_names_folded_and_open_findings_until_the_plan_runs() {
    let fx = revisit_fixture();
    let trace = fx.trace();
    let revisit: Vec<Vec<String>> = trace.entries.iter().map(|e| e.revisit.clone()).collect();
    assert_eq!(revisit, [vec!["F-3"], vec!["F-2", "F-3", "F-4"], vec![]]);
    let revisits: Vec<&Gap> = trace
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::Revisit)
        .collect();
    let second = node(METAPLAN, Some(trace.entries[1].anchor.as_str()));
    assert_eq!(revisits[0].node, node(PLAN_FIRST, None));
    assert_eq!(revisits[0].count, Some(1));
    assert_eq!(revisits[1].node, second);
    assert_eq!(revisits[1].count, Some(3));
    assert_eq!(revisits.len(), 2);
    let triage = trace.gaps.last().unwrap();
    assert_eq!((triage.kind, triage.count), (GapKind::Triage, Some(3)));
    assert_eq!(triage.node, node(LEDGER, None));

    fx.rename(PLAN_FIRST, "doc/plans/demo/IN_PROGRESS-PLAN-first.md");
    let trace = fx.trace();
    assert!(trace.entries[0].revisit.is_empty());
    assert_eq!(trace.entries[1].revisit, ["F-2", "F-3", "F-4"]);
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
fn a_folded_status_naming_no_metaplan_entry_is_a_problem() {
    let ledger = format!(
        "# Findings\n\n{}",
        [
            entry("F-1", "Gone", "vision", "folded metaplan#no-such-entry"),
            entry("F-2", "Title", "vision", "folded metaplan#e"),
            entry("F-3", "Plain", "vision", "folded metaplan"),
            entry(
                "F-4",
                "Fine",
                "vision",
                "folded metaplan#1-first-plan-firstmd"
            ),
        ]
        .concat()
    );
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), "# S1\n\n## One\n\nText.\n"),
        (
            METAPLAN,
            "# E\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01\n",
        ),
        (LEDGER, &ledger),
    ]);
    let project = fx.load();
    let found: Vec<(Option<usize>, &str)> = project
        .problems
        .iter()
        .filter(|p| p.code == "findings-bad-status")
        .map(|p| (p.line, p.message.as_str()))
        .collect();
    assert_eq!(found.len(), 3, "{found:?}");
    let lines: Vec<Option<usize>> = found.iter().map(|(line, _)| *line).collect();
    assert_eq!(lines, [Some(3), Some(10), Some(17)]);
    assert!(
        found[0].0.is_some() && found[0].1.contains("F-1"),
        "{found:?}"
    );
    let trace = &project.feature("demo").unwrap().trace;
    assert_eq!(trace.entries[0].revisit, ["F-4"]);
}
