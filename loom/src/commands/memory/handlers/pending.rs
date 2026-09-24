//! Listing unresolved memory events across the current plan.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::fs::memory::{
    list_journals, read_staged_entries, settled_ids, MemoryEntry, MemoryEntryType,
};

use super::read::{read_journal_with_pending, spool_only_stage_with_pending};
use super::work_dir::{readonly_work_dir, validate_stage_id};

#[path = "pending_groups.rs"]
mod groups;
#[cfg(test)]
#[path = "suggestion_tests.rs"]
mod suggestion_tests;

use groups::{group_pending, print_grouped_human_report};

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

/// Print unresolved notes, decisions, questions, and suggestions.
///
/// `group` sorts pending entries into the buckets a distiller works through
/// in order (`corrections`, `mistakes`, `decisions`, `suggestions`, `other`)
/// per the grouping contract in `doc/plans/briefs/loom-efficiency-and-acceptance/common.md`.
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

pub(super) fn strict_should_fail(strict: bool, pending_count: usize) -> bool {
    strict && pending_count > 0
}

pub(super) fn pending_report(work_dir: &Path, stage_id: Option<&str>) -> Result<PendingReport> {
    let (entries, journals) = staged_entries(work_dir)?;
    let settled = settled_ids(entries.iter().map(|staged| &staged.entry));
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
                && !settled.contains(&staged.entry.id)
        })
        .count();
    let pending = entries
        .into_iter()
        .filter(|staged| {
            in_scope(staged)
                && matches!(
                    staged.entry.entry_type,
                    MemoryEntryType::Note
                        | MemoryEntryType::Decision
                        | MemoryEntryType::Question
                        | MemoryEntryType::Suggestion
                )
                && !settled.contains(&staged.entry.id)
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
    journals.extend(spool_only_stage_with_pending(&journals));
    let (entries, journals) = read_staged_entries(work_dir, journals, read_journal_with_pending)?;
    let entries = entries
        .into_iter()
        .map(|(stage, entry)| StagedMemoryEntry { stage, entry })
        .collect();
    Ok((entries, journals))
}
