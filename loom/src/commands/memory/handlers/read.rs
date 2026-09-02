//! Read-only handlers: `query`, `list`, `show`.

use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::fs::memory::{
    list_journals, query_entries, read_journal, read_pending, MemoryEntryType, MemoryJournal,
};
use crate::git::worktree::find_worktree_root_from_cwd;

use super::super::formatters::{format_entry_compact, format_entry_full};
use super::work_dir::{readonly_work_dir, validate_stage_id};

/// `(worktree_root, stage_id)` for the worktree that owns the current
/// working directory, if cwd is inside one. Shared by every spool lookup on
/// the read path so "which stage does this worktree belong to" has exactly
/// one definition - a worktree's stage id is its directory's basename
/// (`.worktrees/<stage-id>/`).
fn current_worktree_stage() -> Option<(PathBuf, String)> {
    let cwd = std::env::current_dir().ok()?;
    let worktree_root = find_worktree_root_from_cwd(&cwd)?;
    let stage = worktree_root.file_name()?.to_str()?.to_string();
    Some((worktree_root, stage))
}

/// `read_journal`, plus any entries still pending in this worktree's spool.
///
/// An agent that just recorded a note and immediately runs `loom memory
/// list`/`show`/`query` (post-compaction recovery is exactly this sequence)
/// must still see its own entry even though the daemon hasn't drained the
/// spool into the journal file yet. Only applies when cwd is inside the
/// worktree that owns `stage` - reading another stage's journal must not
/// leak a third worktree's spool into it. A `read_pending` failure degrades
/// to the journal alone rather than failing the read, matching how these
/// read-only commands already tolerate a missing state directory (see
/// `work_dir.rs`).
pub(super) fn read_journal_with_pending(work_dir: &Path, stage: &str) -> Result<MemoryJournal> {
    let mut journal = read_journal(work_dir, stage)?;

    let Some((worktree_root, worktree_stage)) = current_worktree_stage() else {
        return Ok(journal);
    };
    if worktree_stage != stage {
        return Ok(journal);
    }

    if let Ok(pending) = read_pending(&worktree_root) {
        journal.entries.extend(pending);
        journal.entries.sort_by_key(|entry| entry.timestamp);
    }

    Ok(journal)
}

/// A worktree's stage id, if it has spooled entries but no journal file yet
/// (i.e. it's missing from `journals`, which only enumerates journal
/// *files*). `None` on any error or absence - this is a best-effort
/// addition to an aggregate listing, not something that should fail it.
pub(super) fn spool_only_stage_with_pending(journals: &[String]) -> Option<String> {
    let (worktree_root, stage) = current_worktree_stage()?;
    if journals.contains(&stage) {
        return None;
    }
    let pending = read_pending(&worktree_root).ok()?;
    if pending.is_empty() {
        return None;
    }
    Some(stage)
}

/// Query memory entries by search term
pub fn query(search: String, stage_id: Option<String>) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let Some(work_dir) = readonly_work_dir() else {
        println!(
            "{} No memory recorded yet (no state directory found)",
            "ℹ".blue()
        );
        return Ok(());
    };

    let stages_to_search: Vec<String> = match stage_id {
        Some(id) => vec![id],
        None => list_journals(&work_dir)?,
    };

    if stages_to_search.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }

    let mut total_results = 0;
    for stage in &stages_to_search {
        total_results += query_stage(&work_dir, stage, &search)?;
    }

    if total_results == 0 {
        println!(
            "{} No entries found matching '{}'",
            "ℹ".blue(),
            search.cyan()
        );
    } else {
        println!("\n{} {} total results", "Found".bold(), total_results);
    }

    Ok(())
}

/// Query one stage's journal and print matches (compact). Returns the match count.
fn query_stage(work_dir: &Path, stage: &str, search: &str) -> Result<usize> {
    let journal = read_journal_with_pending(work_dir, stage)?;
    let results = query_entries(&journal, search);

    if results.is_empty() {
        return Ok(0);
    }

    let count = results.len();
    println!("\n{} ({})", stage.bold(), count);
    println!("{}", "─".repeat(60));

    for entry in &results {
        println!("{}", format_entry_compact(entry));
    }

    Ok(count)
}

/// Print a single stage's journal entries (compact), applying an optional type filter.
///
/// Returns the number of entries displayed (after filtering). A zero return means
/// the journal had no entries matching the filter and nothing was printed.
fn print_journal_entries(
    work_dir: &Path,
    stage: &str,
    type_filter: Option<MemoryEntryType>,
    limit: usize,
) -> Result<usize> {
    let journal = read_journal_with_pending(work_dir, stage)?;

    let entries: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| type_filter.is_none_or(|t| e.entry_type == t))
        .collect();

    if entries.is_empty() {
        return Ok(0);
    }

    println!(
        "\n{} ({} {})",
        stage.bold(),
        entries.len(),
        if entries.len() == 1 {
            "entry"
        } else {
            "entries"
        }
    );
    println!("{}", "─".repeat(60));

    for entry in entries.iter().rev().take(limit) {
        println!("{}", format_entry_compact(entry));
    }

    if entries.len() > limit {
        println!("  {} {} more...", "...".dimmed(), entries.len() - limit);
    }

    Ok(entries.len())
}

/// List memory entries.
///
/// With an explicit `--stage`, lists only that stage's journal. Without one,
/// aggregates every journal in the plan so a running stage sees all memories
/// recorded so far — not just its own. `LOOM_STAGE_ID` no longer scopes `list`;
/// use `--stage` to narrow to a single stage.
pub fn list(stage_id: Option<String>, entry_type: Option<String>) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let Some(work_dir) = readonly_work_dir() else {
        println!(
            "{} No memory recorded yet (no state directory found)",
            "ℹ".blue()
        );
        return Ok(());
    };
    let type_filter: Option<MemoryEntryType> = entry_type.map(|t| t.parse()).transpose()?;

    if let Some(stage) = stage_id {
        return list_single_stage(&work_dir, &stage, type_filter);
    }

    list_all_stages(&work_dir, type_filter)
}

/// Explicit stage: scope to that single journal.
fn list_single_stage(
    work_dir: &Path,
    stage: &str,
    type_filter: Option<MemoryEntryType>,
) -> Result<()> {
    let shown = print_journal_entries(work_dir, stage, type_filter, 20)?;
    if shown == 0 {
        println!(
            "{} No {} entries in memory journal for stage '{}'",
            "ℹ".blue(),
            type_filter
                .map(|t| t.to_string())
                .unwrap_or_else(|| "matching".to_string()),
            stage
        );
    }
    Ok(())
}

/// No explicit stage: aggregate all journals in the plan.
fn list_all_stages(work_dir: &Path, type_filter: Option<MemoryEntryType>) -> Result<()> {
    let mut journals = list_journals(work_dir)?;
    if let Some(spool_only_stage) = spool_only_stage_with_pending(&journals) {
        journals.push(spool_only_stage);
    }

    if journals.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }
    journals.sort();

    let current_stage = std::env::var("LOOM_STAGE_ID").ok();
    println!(
        "{} Plan Memory — {} journal{}",
        "📚".bold(),
        journals.len(),
        if journals.len() == 1 { "" } else { "s" }
    );
    if let Some(ref cur) = current_stage {
        println!("{} {}", "Current stage:".dimmed(), cur.cyan());
    }

    let mut total_shown = 0;
    for stage_name in &journals {
        total_shown += print_journal_entries(work_dir, stage_name, type_filter, 20)?;
    }

    if total_shown == 0 {
        println!(
            "\n{} No {} entries found across {} journal(s)",
            "ℹ".blue(),
            type_filter
                .map(|t| t.to_string())
                .unwrap_or_else(|| "matching".to_string()),
            journals.len()
        );
    } else {
        println!(
            "\n{} {} entr{} across {} journal{}",
            "Total:".bold(),
            total_shown,
            if total_shown == 1 { "y" } else { "ies" },
            journals.len(),
            if journals.len() == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Show full memory journal
pub fn show(stage_id: Option<String>, all: bool) -> Result<()> {
    let Some(work_dir) = readonly_work_dir() else {
        println!(
            "{} No memory recorded yet (no state directory found)",
            "ℹ".blue()
        );
        return Ok(());
    };

    if all {
        return show_all_journals(&work_dir);
    }

    // Validate the RESOLVED stage id, not just an explicitly-passed `--stage`:
    // an unvalidated `LOOM_STAGE_ID` fallback would otherwise bypass the same
    // traversal check applied to `--stage` before it reaches
    // `read_journal`'s path construction (see the matching fix in
    // `handlers/record.rs`).
    let stage = match stage_id {
        Some(id) => id,
        None => std::env::var("LOOM_STAGE_ID")
            .map_err(|_| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?,
    };
    validate_stage_id(&stage)?;

    show_single_journal(&work_dir, &stage)
}

fn show_all_journals(work_dir: &Path) -> Result<()> {
    let mut journals = list_journals(work_dir)?;
    if let Some(spool_only_stage) = spool_only_stage_with_pending(&journals) {
        journals.push(spool_only_stage);
    }

    if journals.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }
    for stage_name in &journals {
        let journal = read_journal_with_pending(work_dir, stage_name)?;
        if journal.entries.is_empty() {
            continue;
        }
        println!("{}", "═".repeat(60));
        println!("{}", format!("Memory Journal: {stage_name}").bold());
        println!("{} entries", journal.entries.len());
        println!("{}", "═".repeat(60));
        for entry in &journal.entries {
            println!("{}", format_entry_full(entry));
        }
        println!();
    }
    Ok(())
}

fn show_single_journal(work_dir: &Path, stage: &str) -> Result<()> {
    let journal = read_journal_with_pending(work_dir, stage)?;

    if journal.entries.is_empty() {
        println!(
            "{} No entries in memory journal for stage '{}'",
            "ℹ".blue(),
            stage
        );
        return Ok(());
    }

    println!("{}", "═".repeat(60));
    println!("{}", format!("Memory Journal: {stage}").bold());
    println!("{} {}", "Stage:".dimmed(), journal.stage_id);
    println!("{} entries", journal.entries.len());
    println!("{}", "═".repeat(60));

    for entry in &journal.entries {
        println!("{}", format_entry_full(entry));
    }

    println!("\n{}", "═".repeat(60));

    Ok(())
}
