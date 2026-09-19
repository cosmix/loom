//! Listing unresolved memory events across the current plan.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

use crate::fs::memory::{list_journals, MemoryEntry, MemoryEntryType};

use super::prefix::NotePrefix;
use super::read::{read_journal_with_pending, spool_only_stage_with_pending};
use super::work_dir::{readonly_work_dir, validate_stage_id};

#[derive(Debug, Clone, Serialize)]
pub(super) struct StagedMemoryEntry {
    pub(super) stage: String,
    #[serde(flatten)]
    pub(super) entry: MemoryEntry,
}

#[derive(Debug, Serialize)]
pub(super) struct PendingReport {
    pub(super) pending: Vec<StagedMemoryEntry>,
    pub(super) changes_without_receipt: usize,
    pub(super) receipts: usize,
    #[serde(skip)]
    pub(super) journals: usize,
}

/// Print unresolved notes, decisions, and questions.
///
/// `group` sorts pending entries into the four buckets a distiller works
/// through in order (`corrections`, `mistakes`, `decisions`, `other`) per
/// the grouping contract in `doc/plans/briefs/loom-efficiency-and-acceptance/common.md`.
pub fn pending(stage_id: Option<String>, json: bool, strict: bool, group: bool) -> Result<()> {
    if let Some(ref stage) = stage_id {
        validate_stage_id(stage)?;
    }

    let Some(work_dir) = readonly_work_dir() else {
        print_no_journals(json);
        return Ok(());
    };
    let report = pending_report(&work_dir, stage_id.as_deref())?;
    if report.journals == 0 {
        print_no_journals(json);
        return Ok(());
    }

    if group {
        let grouped = group_pending(report);
        if json {
            println!("{}", serde_json::to_string(&grouped)?);
        } else {
            print_grouped_human_report(&grouped);
        }
        if strict_should_fail(strict, grouped.pending_count()) {
            std::process::exit(1);
        }
        return Ok(());
    }

    if json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        print_human_report(&report);
    }

    if strict_should_fail(strict, report.pending.len()) {
        std::process::exit(1);
    }
    Ok(())
}

fn print_no_journals(json: bool) {
    if json {
        println!(r#"{{"pending":[]}}"#);
    } else {
        println!("no memory journals");
    }
}

/// Flatten a note's content to a single line, truncated for a terminal
/// column.
fn preview(content: &str) -> String {
    content
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                ' '
            } else {
                character
            }
        })
        .take(80)
        .collect()
}

fn format_entry_line(staged: &StagedMemoryEntry) -> String {
    format!(
        "{}  {}  {}  {}",
        staged.entry.id,
        staged.entry.entry_type,
        staged.stage,
        preview(&staged.entry.content)
    )
}

fn print_human_report(report: &PendingReport) {
    for pending in &report.pending {
        println!("{}", format_entry_line(pending));
    }
    println!(
        "{} pending across {} journals",
        report.pending.len(),
        report.journals
    );
    println!(
        "changes without receipts: {}",
        report.changes_without_receipt
    );
}

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

/// Pending entries sorted into the four buckets a distiller works through
/// in order: corrections, then mistakes, decisions, other.
#[derive(Debug, Serialize)]
pub(super) struct GroupedPendingReport {
    pub(super) corrections: Vec<CorrectionEntry>,
    pub(super) mistakes: Vec<StagedMemoryEntry>,
    pub(super) decisions: Vec<StagedMemoryEntry>,
    pub(super) other: Vec<StagedMemoryEntry>,
    pub(super) changes_without_receipt: usize,
    pub(super) receipts: usize,
    #[serde(skip)]
    pub(super) journals: usize,
}

impl GroupedPendingReport {
    pub(super) fn pending_count(&self) -> usize {
        self.corrections.len() + self.mistakes.len() + self.decisions.len() + self.other.len()
    }
}

/// Sort a flat [`PendingReport`] into the four groups the memory-grouping
/// contract defines: `corrections` (`stale-knowledge:`, with its
/// `<file>#<heading>` target parsed out), `mistakes` (`mistake:`),
/// `decisions` (entry type `Decision`), and `other` (everything else -
/// including a `found/gotcha:` note, which carries no group of its own).
fn group_pending(report: PendingReport) -> GroupedPendingReport {
    let mut corrections = Vec::new();
    let mut mistakes = Vec::new();
    let mut decisions = Vec::new();
    let mut other = Vec::new();

    for staged in report.pending {
        match NotePrefix::parse(&staged.entry.content) {
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
            _ if staged.entry.entry_type == MemoryEntryType::Decision => decisions.push(staged),
            _ => other.push(staged),
        }
    }

    corrections.sort_by(|a, b| (&a.file, &a.heading).cmp(&(&b.file, &b.heading)));

    GroupedPendingReport {
        corrections,
        mistakes,
        decisions,
        other,
        changes_without_receipt: report.changes_without_receipt,
        receipts: report.receipts,
        journals: report.journals,
    }
}

fn print_grouped_human_report(grouped: &GroupedPendingReport) {
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

pub(super) fn strict_should_fail(strict: bool, pending_count: usize) -> bool {
    strict && pending_count > 0
}

pub(super) fn pending_report(work_dir: &Path, stage_id: Option<&str>) -> Result<PendingReport> {
    let (entries, journals) = staged_entries(work_dir)?;
    let settled_ids: HashSet<String> = entries
        .iter()
        .filter_map(|staged| staged.entry.receipt.as_ref())
        .map(|receipt| receipt.event_id.clone())
        .collect();
    let receipts = entries
        .iter()
        .filter(|staged| staged.entry.entry_type == MemoryEntryType::Receipt)
        .count();
    let in_scope =
        |staged: &StagedMemoryEntry| stage_id.is_none_or(|stage| staged.stage.as_str() == stage);
    let changes_without_receipt = entries
        .iter()
        .filter(|staged| {
            in_scope(staged)
                && staged.entry.entry_type == MemoryEntryType::Change
                && !settled_ids.contains(&staged.entry.id)
        })
        .count();
    let pending = entries
        .into_iter()
        .filter(|staged| {
            in_scope(staged)
                && matches!(
                    staged.entry.entry_type,
                    MemoryEntryType::Note | MemoryEntryType::Decision | MemoryEntryType::Question
                )
                && !settled_ids.contains(&staged.entry.id)
        })
        .collect();

    let journals = match stage_id {
        Some(stage) if journals.iter().any(|journal| journal == stage) => 1,
        Some(_) => 0,
        None => journals.len(),
    };
    Ok(PendingReport {
        pending,
        changes_without_receipt,
        receipts,
        journals,
    })
}

/// Read every journal once, adding only the current worktree's undrained spool.
pub(super) fn staged_entries(work_dir: &Path) -> Result<(Vec<StagedMemoryEntry>, Vec<String>)> {
    let mut journals = list_journals(work_dir)?;
    if let Some(stage) = spool_only_stage_with_pending(&journals) {
        journals.push(stage);
    }
    journals.sort();
    journals.dedup();

    let mut seen_ids = HashSet::new();
    let mut entries = Vec::new();
    for stage in &journals {
        let journal = read_journal_with_pending(work_dir, stage)?;
        entries.extend(
            journal
                .entries
                .into_iter()
                .filter(|entry| seen_ids.insert(entry.id.clone()))
                .map(|entry| StagedMemoryEntry {
                    stage: stage.clone(),
                    entry,
                }),
        );
    }
    Ok((entries, journals))
}

#[cfg(test)]
mod group_tests {
    use super::*;

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
}
