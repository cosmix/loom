//! Tests for the run review's bullet sections.

use super::*;
use crate::fs::memory::{MemoryEntryType, Receipt, ReceiptOutcome};
use chrono::{Duration, TimeZone};

fn stages(specs: &[(&str, &str)]) -> Vec<StageInfo> {
    let dir = tempfile::tempdir().unwrap();
    for (index, (id, name)) in specs.iter().enumerate() {
        let body = format!("---\nid: {id}\nname: {name}\n---\n");
        std::fs::write(dir.path().join(format!("{index:02}-{id}.md")), body).unwrap();
    }
    load_stage_infos(dir.path()).unwrap()
}

fn entry(kind: MemoryEntryType, text: &str) -> MemoryEntry {
    MemoryEntry::new(kind, text.into())
}

fn receipt(id: &str, outcome: ReceiptOutcome, minute: i64) -> MemoryEntry {
    let receipt = Receipt {
        event_id: id.into(),
        outcome,
        target: None,
    };
    let mut entry = MemoryEntry::receipt(receipt, String::new());
    let base = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap();
    entry.timestamp = base + Duration::minutes(minute);
    entry
}

fn findings(stages: &[StageInfo], journals: &Journals) -> String {
    let mut doc = String::new();
    append_open_findings(&mut doc, stages, journals);
    doc
}

#[test]
fn open_findings_select_by_outcome() {
    use MemoryEntryType as K;
    use ReceiptOutcome as O;
    let stages = stages(&[("build", "Build")]);
    let kinds_and_outcomes = [
        (K::Suggestion, None, true),
        (K::Suggestion, Some(O::Promoted), true),
        (K::Suggestion, Some(O::Merged), true),
        (K::Suggestion, Some(O::Deferred), true),
        (K::Suggestion, Some(O::Implemented), false),
        (K::Suggestion, Some(O::Discarded), false),
        (K::Note, Some(O::Deferred), true),
        (K::Decision, Some(O::Deferred), true),
        (K::Note, Some(O::Promoted), false),
        (K::Note, None, false),
        (K::Question, Some(O::Deferred), false),
        (K::Change, Some(O::Deferred), false),
    ];
    let mut built = Vec::new();
    let mut receipts = Vec::new();
    let mut expected = Vec::new();
    for (index, (kind, outcome, open)) in kinds_and_outcomes.into_iter().enumerate() {
        let item = entry(kind, &format!("item {index}"));
        if let Some(outcome) = outcome {
            receipts.push(receipt(&item.id, outcome, 0));
        }
        if open {
            expected.push(review_item_line("Build", "build", &item.content));
        }
        built.push(item);
    }
    let journals = Journals::from([
        ("build".to_string(), built),
        ("knowledge-distill".to_string(), receipts),
    ]);
    let doc = findings(&stages, &journals);
    let lines: Vec<&str> = doc.lines().filter(|l| l.starts_with("- ")).collect();
    assert_eq!(lines, expected, "{doc}");
}

#[test]
fn the_latest_receipt_decides_across_journals() {
    let stages = stages(&[("build", "Build")]);
    let reopened = entry(MemoryEntryType::Suggestion, "reopened");
    let closed = entry(MemoryEntryType::Suggestion, "closed");
    // The newer receipt sits in the journal visited first, so order cannot decide.
    let early = vec![
        receipt(&reopened.id, ReceiptOutcome::Deferred, 5),
        receipt(&closed.id, ReceiptOutcome::Deferred, 1),
    ];
    let late = vec![
        receipt(&reopened.id, ReceiptOutcome::Implemented, 1),
        receipt(&closed.id, ReceiptOutcome::Implemented, 5),
    ];
    let journals = Journals::from([
        ("build".to_string(), vec![reopened, closed]),
        ("a-review".to_string(), early),
        ("z-distill".to_string(), late),
    ]);
    let doc = findings(&stages, &journals);
    assert!(doc.contains("(`build`): reopened\n"), "{doc}");
    assert!(!doc.contains("closed"), "{doc}");
}

#[test]
fn review_item_line_escapes_the_name_and_collapses_the_text() {
    assert_eq!(
        review_item_line("A \\ *b* c", "s-1", "  one\n\ttwo   three \n"),
        "- **A \\\\ \\*b\\* c** (`s-1`): one two three"
    );
}

#[test]
fn a_multi_line_stage_name_stays_one_bullet_the_parser_reads_back() {
    use crate::planning::run_review::{review_items, ReviewSection};
    let line = review_item_line("Wire the\n  *bridge*", "wire", "done");
    assert_eq!(line, "- **Wire the \\*bridge\\*** (`wire`): done");
    let doc = format!("{OPEN_QUESTIONS_HEADING}\n\n{line}\n");
    let items = review_items(&doc, ReviewSection::OpenQuestions);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].stage_name, "Wire the *bridge*");
    assert_eq!(
        (items[0].stage_id.as_str(), items[0].text.as_str()),
        ("wire", "done")
    );
}

#[test]
fn spec_conformance_keeps_tagged_decisions_and_questions_in_stage_order() {
    let stages = stages(&[("build", "Build"), ("integration-verify", "Verify")]);
    let tag = SPEC_CONFORMANCE_PREFIX;
    let journals = Journals::from([
        (
            "integration-verify".to_string(),
            vec![
                entry(MemoryEntryType::Decision, &format!("{tag} s_01: fixed")),
                entry(MemoryEntryType::Note, &format!("{tag} ignored note")),
                entry(MemoryEntryType::Question, &format!("{tag} s_02: gap")),
                entry(MemoryEntryType::Decision, "untagged decision"),
            ],
        ),
        (
            "build".to_string(),
            vec![entry(MemoryEntryType::Question, &format!("{tag} early"))],
        ),
    ]);
    let mut doc = String::new();
    append_spec_conformance(&mut doc, &stages, &journals);
    assert_eq!(
        doc,
        format!(
            "{SPEC_CONFORMANCE_HEADING}\n\n- **Build** (`build`): early\n\
             - **Verify** (`integration-verify`): s_01: fixed\n\
             - **Verify** (`integration-verify`): s_02: gap\n\n"
        )
    );
}

#[test]
fn empty_sections_say_so_in_order() {
    let stages = stages(&[("build", "Build")]);
    let doc = render("PLAN-x", "X", "About X.", &stages, &Journals::new());
    let expected = [
        "## Summary\n\nAbout X.",
        "## Changes by Stage",
        "## Spec Conformance\n\nNo spec conformance outcomes.\n\n",
        "## Open Questions\n\nNo open questions.\n\n",
        "## Open Findings\n\nNo open findings.\n\n",
    ];
    let mut from = 0;
    for part in expected {
        let found = doc[from..].find(part);
        let at = found.unwrap_or_else(|| panic!("{part}:\n{doc}"));
        from += at + part.len();
    }
}
