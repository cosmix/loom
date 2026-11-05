//! The run review's bullet sections: Spec Conformance, Open Questions and Open Findings.
//!
//! Every bullet is written by [`review_item_line`]; `planning::run_review` parses them back.

use super::stages::StageInfo;
use super::Journals;
use crate::fs::memory::{MemoryEntry, MemoryEntryType, ReceiptOutcome};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// The heading of the spec conformance outcomes integration-verify recorded.
pub const SPEC_CONFORMANCE_HEADING: &str = "## Spec Conformance";
/// The heading of the questions recorded during the run.
pub const OPEN_QUESTIONS_HEADING: &str = "## Open Questions";
/// The heading of the suggestions never built and the entries deferred during the run.
pub const OPEN_FINDINGS_HEADING: &str = "## Open Findings";
/// The prefix integration-verify gives the decisions and questions that record spec outcomes.
pub const SPEC_CONFORMANCE_PREFIX: &str = "spec-conformance:";

/// One bullet in the shared format: ``- **<stage name>** (`<stage-id>`): <text>``.
///
/// The name escapes `\` and `*` with a backslash; every whitespace run in the name and in the
/// text, newlines included, collapses to one space, trimmed. No trailing newline.
pub fn review_item_line(stage_name: &str, stage_id: &str, text: &str) -> String {
    let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = collapse(stage_name)
        .replace('\\', "\\\\")
        .replace('*', "\\*");
    let text = collapse(text);
    format!("- **{name}** (`{stage_id}`): {text}")
}

/// List the `spec-conformance:` decisions and questions, prefix removed.
pub(super) fn append_spec_conformance(doc: &mut String, stages: &[StageInfo], journals: &Journals) {
    let items = select(stages, journals, |entry| {
        let spec_kind = matches!(
            entry.entry_type,
            MemoryEntryType::Decision | MemoryEntryType::Question
        );
        entry
            .content
            .strip_prefix(SPEC_CONFORMANCE_PREFIX)
            .filter(|_| spec_kind)
    });
    append_section(
        doc,
        SPEC_CONFORMANCE_HEADING,
        "No spec conformance outcomes.",
        &items,
    );
}

/// List every question recorded during the run.
pub(super) fn append_questions(doc: &mut String, stages: &[StageInfo], journals: &Journals) {
    let items = select(stages, journals, |entry| {
        let question = entry.entry_type == MemoryEntryType::Question;
        question.then_some(entry.content.as_str())
    });
    append_section(doc, OPEN_QUESTIONS_HEADING, "No open questions.", &items);
}

/// List the suggestions never implemented or discarded, and the notes and decisions deferred.
///
/// The run review is written after knowledge-distill resolves every entry, so "still pending"
/// would always be empty; the receipt outcome decides instead.
pub(super) fn append_open_findings(doc: &mut String, stages: &[StageInfo], journals: &Journals) {
    let outcomes = latest_outcomes(journals);
    let items = select(stages, journals, |entry| {
        let outcome = outcomes.get(entry.id.as_str()).copied();
        let open = is_open_finding(entry.entry_type, outcome);
        open.then_some(entry.content.as_str())
    });
    append_section(doc, OPEN_FINDINGS_HEADING, "No open findings.", &items);
}

/// A suggestion is open until implemented or discarded; a note or decision only when deferred.
fn is_open_finding(kind: MemoryEntryType, outcome: Option<ReceiptOutcome>) -> bool {
    use ReceiptOutcome::{Deferred, Discarded, Implemented};
    match kind {
        MemoryEntryType::Suggestion => !matches!(outcome, Some(Implemented | Discarded)),
        MemoryEntryType::Note | MemoryEntryType::Decision => outcome == Some(Deferred),
        _ => false,
    }
}

/// Each settled entry's outcome: its receipt with the latest timestamp across every journal.
///
/// Journals are visited by name so a timestamp tie resolves the same way on every run; on a tie
/// the receipt visited last wins.
fn latest_outcomes(journals: &Journals) -> HashMap<&str, ReceiptOutcome> {
    let mut names: Vec<&String> = journals.keys().collect();
    names.sort();
    let mut latest: HashMap<&str, (DateTime<Utc>, ReceiptOutcome)> = HashMap::new();
    for entry in names.into_iter().flat_map(|name| &journals[name]) {
        let Some(receipt) = &entry.receipt else {
            continue;
        };
        let id = receipt.event_id.as_str();
        if latest.get(id).is_none_or(|(at, _)| *at <= entry.timestamp) {
            latest.insert(id, (entry.timestamp, receipt.outcome));
        }
    }
    latest
        .into_iter()
        .map(|(id, (_, outcome))| (id, outcome))
        .collect()
}

/// The `(stage name, stage id, text)` of each entry `pick` keeps, in stage then journal order.
fn select<'a>(
    stages: &'a [StageInfo],
    journals: &'a Journals,
    pick: impl Fn(&'a MemoryEntry) -> Option<&'a str>,
) -> Vec<(&'a str, &'a str, &'a str)> {
    let mut items = Vec::new();
    for stage in stages {
        let Some(entries) = journals.get(&stage.id) else {
            continue;
        };
        for text in entries.iter().filter_map(&pick) {
            items.push((stage.name.as_str(), stage.id.as_str(), text));
        }
    }
    items
}

fn append_section(doc: &mut String, heading: &str, empty: &str, items: &[(&str, &str, &str)]) {
    doc.push_str(heading);
    doc.push_str("\n\n");
    if items.is_empty() {
        doc.push_str(empty);
        doc.push('\n');
    }
    for (name, id, text) in items {
        doc.push_str(&review_item_line(name, id, text));
        doc.push('\n');
    }
    doc.push('\n');
}
