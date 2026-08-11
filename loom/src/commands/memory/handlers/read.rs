//! Read-only handlers: `query`, `list`, `show`.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use crate::fs::memory::{list_journals, query_entries, read_journal, MemoryEntryType};

use super::super::formatters::{format_entry_compact, format_entry_full};
use super::work_dir::{get_work_dir_readonly, validate_stage_id};

/// Query memory entries by search term
pub fn query(search: String, stage_id: Option<String>) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let Some(work_dir) = get_work_dir_readonly() else {
        println!(
            "{} No memory recorded yet (no .work directory found)",
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
    let journal = read_journal(work_dir, stage)?;
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
    let journal = read_journal(work_dir, stage)?;

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

    let Some(work_dir) = get_work_dir_readonly() else {
        println!(
            "{} No memory recorded yet (no .work directory found)",
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
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let Some(work_dir) = get_work_dir_readonly() else {
        println!(
            "{} No memory recorded yet (no .work directory found)",
            "ℹ".blue()
        );
        return Ok(());
    };

    if all {
        return show_all_journals(&work_dir);
    }

    let stage = match stage_id {
        Some(id) => id,
        None => std::env::var("LOOM_STAGE_ID")
            .map_err(|_| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?,
    };

    show_single_journal(&work_dir, &stage)
}

fn show_all_journals(work_dir: &Path) -> Result<()> {
    let journals = list_journals(work_dir)?;
    if journals.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }
    for stage_name in &journals {
        let journal = read_journal(work_dir, stage_name)?;
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
    let journal = read_journal(work_dir, stage)?;

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
