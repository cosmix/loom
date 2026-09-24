//! Sorting pending memory events into the buckets a distiller works through.

use serde::Serialize;

use crate::fs::memory::MemoryEntryType;

use super::super::prefix::NotePrefix;
use super::{format_entry_line, preview, PendingReport, StagedMemoryEntry};

/// A pending `stale-knowledge:` note, with the `<file>#<heading>` target
/// it names broken out into its own column so a distiller can walk the
/// corrections file by file, applying each with `replace-section`.
#[derive(Debug, Serialize)]
pub(super) struct CorrectionEntry {
    pub(super) target: String,
    pub(super) file: String,
    pub(super) heading: String,
    #[serde(flatten)]
    pub(super) staged: StagedMemoryEntry,
}

/// Pending entries sorted into the buckets a distiller works through in
/// order: corrections, then mistakes, decisions, suggestions, other.
#[derive(Debug, Serialize)]
pub(super) struct GroupedPendingReport {
    pub(super) corrections: Vec<CorrectionEntry>,
    pub(super) mistakes: Vec<StagedMemoryEntry>,
    pub(super) decisions: Vec<StagedMemoryEntry>,
    pub(super) suggestions: Vec<StagedMemoryEntry>,
    pub(super) other: Vec<StagedMemoryEntry>,
    pub(super) changes_without_receipt: usize,
    pub(super) receipts: usize,
    #[serde(skip)]
    pub(super) journals: usize,
}

impl GroupedPendingReport {
    pub(super) fn pending_count(&self) -> usize {
        self.corrections.len()
            + self.mistakes.len()
            + self.decisions.len()
            + self.suggestions.len()
            + self.other.len()
    }
}

/// Sort a flat [`PendingReport`] into the groups the memory-grouping
/// contract defines: `suggestions` (entry type `Suggestion`, whatever its
/// text opens with), `corrections` (`stale-knowledge:`, with its
/// `<file>#<heading>` target parsed out), `mistakes` (`mistake:`),
/// `decisions` (entry type `Decision`), and `other` (everything else -
/// including a `found/gotcha:` note, which carries no group of its own).
pub(super) fn group_pending(report: PendingReport) -> GroupedPendingReport {
    let mut corrections = Vec::new();
    let mut mistakes = Vec::new();
    let mut decisions = Vec::new();
    let mut suggestions = Vec::new();
    let mut other = Vec::new();

    for staged in report.pending {
        let entry_type = staged.entry.entry_type;
        match NotePrefix::parse(&staged.entry.content) {
            _ if entry_type == MemoryEntryType::Suggestion => suggestions.push(staged),
            NotePrefix::StaleKnowledge { file, heading } => {
                let target = format!("{file}#{heading}");
                corrections.push(CorrectionEntry {
                    target,
                    file,
                    heading,
                    staged,
                });
            }
            NotePrefix::Mistake => mistakes.push(staged),
            _ if entry_type == MemoryEntryType::Decision => decisions.push(staged),
            _ => other.push(staged),
        }
    }

    corrections.sort_by(|a, b| (&a.file, &a.heading).cmp(&(&b.file, &b.heading)));

    GroupedPendingReport {
        corrections,
        mistakes,
        decisions,
        suggestions,
        other,
        changes_without_receipt: report.changes_without_receipt,
        receipts: report.receipts,
        journals: report.journals,
    }
}

pub(super) fn print_grouped_human_report(grouped: &GroupedPendingReport) {
    println!("corrections:");
    for correction in &grouped.corrections {
        println!(
            "  {}  {}  {}  {}",
            correction.staged.entry.id,
            correction.staged.stage,
            correction.target,
            preview(&correction.staged.entry.content)
        );
    }
    for (label, entries) in [
        ("mistakes", &grouped.mistakes),
        ("decisions", &grouped.decisions),
        ("suggestions", &grouped.suggestions),
        ("other", &grouped.other),
    ] {
        println!("{label}:");
        for staged in entries {
            println!("  {}", format_entry_line(staged));
        }
    }
    println!(
        "{} pending across {} journals",
        grouped.pending_count(),
        grouped.journals
    );
    println!(
        "changes without receipts: {}",
        grouped.changes_without_receipt
    );
}

#[cfg(test)]
mod group_tests {
    use super::*;
    use crate::fs::memory::MemoryEntry;

    fn staged(entry_type: MemoryEntryType, content: &str) -> StagedMemoryEntry {
        StagedMemoryEntry {
            stage: "stage-a".to_string(),
            entry: MemoryEntry::new(entry_type, content.to_string()),
        }
    }

    fn report(pending: Vec<StagedMemoryEntry>) -> PendingReport {
        PendingReport {
            pending,
            changes_without_receipt: 0,
            receipts: 0,
            journals: 1,
        }
    }

    #[test]
    fn sorts_entries_into_the_four_groups() {
        let grouped = group_pending(report(vec![
            staged(
                MemoryEntryType::Note,
                "stale-knowledge: b.md#Heading claims X; the tree does Y. Correction: fix",
            ),
            staged(
                MemoryEntryType::Note,
                "mistake: tried X. Failed. Prevention: check. Fix: did it",
            ),
            staged(MemoryEntryType::Decision, "chose X over Y"),
            staged(MemoryEntryType::Question, "what about Z?"),
        ]));

        assert_eq!(grouped.corrections.len(), 1);
        assert_eq!(grouped.corrections[0].target, "b.md#Heading");
        assert_eq!(grouped.mistakes.len(), 1);
        assert_eq!(grouped.decisions.len(), 1);
        assert_eq!(grouped.other.len(), 1);
        assert_eq!(grouped.pending_count(), 4);
    }

    #[test]
    fn corrections_are_sorted_by_file_then_heading() {
        let grouped = group_pending(report(vec![
            staged(
                MemoryEntryType::Note,
                "stale-knowledge: b.md#Zeta claims X; the tree does Y. Correction: fix",
            ),
            staged(
                MemoryEntryType::Note,
                "stale-knowledge: a.md#Beta claims X; the tree does Y. Correction: fix",
            ),
            staged(
                MemoryEntryType::Note,
                "stale-knowledge: a.md#Alpha claims X; the tree does Y. Correction: fix",
            ),
        ]));

        let targets: Vec<&str> = grouped
            .corrections
            .iter()
            .map(|correction| correction.target.as_str())
            .collect();
        assert_eq!(targets, vec!["a.md#Alpha", "a.md#Beta", "b.md#Zeta"]);
    }

    #[test]
    fn a_note_with_no_recognized_prefix_falls_to_other() {
        let grouped = group_pending(report(vec![staged(MemoryEntryType::Note, "plain note")]));
        assert_eq!(grouped.other.len(), 1);
        assert!(grouped.corrections.is_empty());
        assert!(grouped.mistakes.is_empty());
    }

    #[test]
    fn a_suggestion_keeps_its_own_group_whatever_its_text_opens_with() {
        let suggestion = staged(MemoryEntryType::Suggestion, "mistake: tried X");
        let grouped = group_pending(report(vec![suggestion]));
        assert_eq!(grouped.suggestions.len(), 1);
        assert!(grouped.mistakes.is_empty());
        assert_eq!(grouped.pending_count(), 1);
    }
}
