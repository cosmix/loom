//! Command handler implementations for memory subcommands.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::env;

use crate::fs::memory::{
    append_entry, list_journals, query_entries, read_journal, validate_content, MemoryEntry,
    MemoryEntryType,
};
use crate::git::worktree::{find_repo_root_from_cwd, find_worktree_root_from_cwd};

use super::formatters::{
    format_entry_compact, format_entry_full, format_record_success, format_stage_summary,
};

/// Get the .work directory, handling worktree symlinks
///
/// When called from within a worktree (or its subdirectory), finds the worktree root
/// which has a `.work` symlink pointing to the main repo's `.work`.
/// When called from the main repo, walks up to find the repo root's `.work`.
fn get_work_dir() -> Result<std::path::PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;

    // First check if we're in a worktree
    if let Some(worktree_root) = find_worktree_root_from_cwd(&cwd) {
        let work_dir = worktree_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Not in a worktree (or worktree missing .work) - find repo root
    if let Some(repo_root) = find_repo_root_from_cwd(&cwd) {
        let work_dir = repo_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Fallback: check current directory (original behavior)
    let work_dir = cwd.join(".work");
    if work_dir.exists() {
        return Ok(work_dir);
    }

    bail!(".work directory not found. Run 'loom init' first.");
}

/// Validate stage ID to prevent path traversal attacks
fn validate_stage_id(id: &str) -> Result<()> {
    if id.contains('/') || id.contains("..") || id.contains('\\') {
        bail!("Invalid stage ID: contains path separators");
    }
    Ok(())
}

/// Record a note in the memory journal
pub fn note(text: String, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .ok_or_else(|| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?;

    let entry = MemoryEntry::new(MemoryEntryType::Note, text.clone());
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Note, &stage, &text)
    );

    Ok(())
}

/// Record a decision in the memory journal
pub fn decision(text: String, context: Option<String>, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref ctx) = context {
        validate_content(ctx)?;
    }
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .ok_or_else(|| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?;

    let entry = match context {
        Some(ctx) => MemoryEntry::with_context(MemoryEntryType::Decision, text.clone(), ctx),
        None => MemoryEntry::new(MemoryEntryType::Decision, text.clone()),
    };
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Decision, &stage, &text)
    );

    Ok(())
}

/// Record a file change in the memory journal
pub fn change(text: String, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .ok_or_else(|| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?;

    let entry = MemoryEntry::new(MemoryEntryType::Change, text.clone());
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Change, &stage, &text)
    );

    Ok(())
}

/// Record a question in the memory journal
pub fn question(text: String, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .ok_or_else(|| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?;

    let entry = MemoryEntry::new(MemoryEntryType::Question, text.clone());
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Question, &stage, &text)
    );

    Ok(())
}

/// Query memory entries by search term
pub fn query(search: String, stage_id: Option<String>) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_work_dir()?;

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
        let journal = read_journal(&work_dir, stage)?;
        let results = query_entries(&journal, &search);

        if results.is_empty() {
            continue;
        }

        let count = results.len();
        println!("\n{} ({})", stage.bold(), count);
        println!("{}", "─".repeat(60));

        for entry in &results {
            println!("{}", format_entry_compact(entry));
        }

        total_results += count;
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

/// List memory entries from a stage
pub fn list(stage_id: Option<String>, entry_type: Option<String>) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_work_dir()?;

    let stage = match stage_id {
        Some(id) => id,
        None => match std::env::var("LOOM_STAGE_ID").ok() {
            Some(id) => id,
            None => {
                // No stage specified - show summary of all stages
                let journals = list_journals(&work_dir)?;
                if journals.is_empty() {
                    println!("{} No memory journals found", "ℹ".blue());
                    return Ok(());
                }
                println!("{} Memory Journals ({})", "📚".bold(), journals.len());
                println!("{}", "─".repeat(60));
                for stage_name in &journals {
                    let journal = read_journal(&work_dir, stage_name)?;
                    let notes = journal
                        .entries
                        .iter()
                        .filter(|e| e.entry_type == MemoryEntryType::Note)
                        .count();
                    let decisions = journal
                        .entries
                        .iter()
                        .filter(|e| e.entry_type == MemoryEntryType::Decision)
                        .count();
                    let questions = journal
                        .entries
                        .iter()
                        .filter(|e| e.entry_type == MemoryEntryType::Question)
                        .count();
                    let changes = journal
                        .entries
                        .iter()
                        .filter(|e| e.entry_type == MemoryEntryType::Change)
                        .count();
                    println!(
                        "{}",
                        format_stage_summary(
                            stage_name,
                            journal.entries.len(),
                            notes,
                            decisions,
                            questions,
                            changes
                        )
                    );
                }
                return Ok(());
            }
        },
    };

    let journal = read_journal(&work_dir, &stage)?;

    if journal.entries.is_empty() {
        println!(
            "{} No entries in memory journal for stage '{}'",
            "ℹ".blue(),
            stage
        );
        return Ok(());
    }

    // Filter by type if specified
    let type_filter: Option<MemoryEntryType> = entry_type.map(|t| t.parse()).transpose()?;

    let entries: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| type_filter.is_none_or(|t| e.entry_type == t))
        .collect();

    if entries.is_empty() {
        println!(
            "{} No {} entries found in stage '{}'",
            "ℹ".blue(),
            type_filter
                .map(|t| t.to_string())
                .unwrap_or_else(|| "matching".to_string()),
            stage
        );
        return Ok(());
    }

    println!(
        "\n{} Memory Journal ({} entries)",
        stage.bold(),
        entries.len()
    );
    println!("{} {}", "Stage:".dimmed(), &journal.stage_id);
    println!("{}", "─".repeat(60));

    for entry in entries.iter().rev().take(20) {
        println!("{}", format_entry_compact(entry));
    }

    if entries.len() > 20 {
        println!("  {} {} more...", "...".dimmed(), entries.len() - 20);
    }

    Ok(())
}

/// Show full memory journal
pub fn show(stage_id: Option<String>, all: bool) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_work_dir()?;

    if all {
        let journals = list_journals(&work_dir)?;
        if journals.is_empty() {
            println!("{} No memory journals found", "ℹ".blue());
            return Ok(());
        }
        for stage_name in &journals {
            let journal = read_journal(&work_dir, stage_name)?;
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
        return Ok(());
    }

    let stage = match stage_id {
        Some(id) => id,
        None => std::env::var("LOOM_STAGE_ID")
            .map_err(|_| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?,
    };

    let journal = read_journal(&work_dir, &stage)?;

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
    println!("{} {}", "Stage:".dimmed(), &journal.stage_id);
    println!("{} entries", journal.entries.len());
    println!("{}", "═".repeat(60));

    for entry in &journal.entries {
        println!("{}", format_entry_full(entry));
    }

    println!("\n{}", "═".repeat(60));

    Ok(())
}
