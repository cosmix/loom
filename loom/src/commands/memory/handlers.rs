//! Command handler implementations for memory subcommands.

use anyhow::{Context, Result};
use colored::Colorize;
use std::env;

use crate::commands::common::{detect_session_from_signals, truncate_for_display};
use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::fs::memory::{
    append_entry, delete_entries_by_type, list_journals, query_entries, read_journal,
    validate_content, MemoryEntry, MemoryEntryType,
};

use super::formatters::{
    format_entries_for_knowledge, format_entry_compact, format_entry_full,
    format_record_success, format_session_summary,
};

/// Get the .work directory, handling worktree symlinks
fn get_work_dir() -> Result<std::path::PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;
    let work_dir = cwd.join(".work");

    if !work_dir.exists() {
        anyhow::bail!(".work directory not found. Run 'loom init' first.");
    }

    Ok(work_dir)
}

/// Record a note in the memory journal
pub fn note(text: String, session_id: Option<String>) -> Result<()> {
    validate_content(&text)?;

    let work_dir = get_work_dir()?;
    let session = session_id
        .or_else(|| detect_session_from_signals(&work_dir).ok())
        .ok_or_else(|| anyhow::anyhow!("No session ID provided or detected. Use --session <id>"))?;

    let entry = MemoryEntry::new(MemoryEntryType::Note, text.clone());
    append_entry(&work_dir, &session, &entry)?;

    println!("{}", format_record_success(&MemoryEntryType::Note, &session, &text));

    Ok(())
}

/// Record a decision in the memory journal
pub fn decision(text: String, context: Option<String>, session_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref ctx) = context {
        validate_content(ctx)?;
    }

    let work_dir = get_work_dir()?;
    let session = session_id
        .or_else(|| detect_session_from_signals(&work_dir).ok())
        .ok_or_else(|| anyhow::anyhow!("No session ID provided or detected. Use --session <id>"))?;

    let entry = match context {
        Some(ctx) => MemoryEntry::with_context(MemoryEntryType::Decision, text.clone(), ctx),
        None => MemoryEntry::new(MemoryEntryType::Decision, text.clone()),
    };
    append_entry(&work_dir, &session, &entry)?;

    println!("{}", format_record_success(&MemoryEntryType::Decision, &session, &text));

    Ok(())
}

/// Record a question in the memory journal
pub fn question(text: String, session_id: Option<String>) -> Result<()> {
    validate_content(&text)?;

    let work_dir = get_work_dir()?;
    let session = session_id
        .or_else(|| detect_session_from_signals(&work_dir).ok())
        .ok_or_else(|| anyhow::anyhow!("No session ID provided or detected. Use --session <id>"))?;

    let entry = MemoryEntry::new(MemoryEntryType::Question, text.clone());
    append_entry(&work_dir, &session, &entry)?;

    println!("{}", format_record_success(&MemoryEntryType::Question, &session, &text));

    Ok(())
}

/// Query memory entries by search term
pub fn query(search: String, session_id: Option<String>) -> Result<()> {
    let work_dir = get_work_dir()?;

    let sessions_to_search: Vec<String> = match session_id {
        Some(id) => vec![id],
        None => list_journals(&work_dir)?,
    };

    if sessions_to_search.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }

    let mut total_results = 0;

    for session in &sessions_to_search {
        let journal = read_journal(&work_dir, session)?;
        let results = query_entries(&journal, &search);

        if results.is_empty() {
            continue;
        }

        let count = results.len();
        println!("\n{} ({})", session.bold(), count);
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

/// List memory entries from a session
pub fn list(session_id: Option<String>, entry_type: Option<String>) -> Result<()> {
    let work_dir = get_work_dir()?;

    let session = match session_id {
        Some(id) => id,
        None => detect_session_from_signals(&work_dir)?,
    };

    let journal = read_journal(&work_dir, &session)?;

    if journal.entries.is_empty() {
        println!(
            "{} No entries in memory journal for session '{}'",
            "ℹ".blue(),
            session
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
            "{} No {} entries found in session '{}'",
            "ℹ".blue(),
            type_filter
                .map(|t| t.to_string())
                .unwrap_or_else(|| "matching".to_string()),
            session
        );
        return Ok(());
    }

    println!(
        "\n{} Memory Journal ({} entries)",
        session.bold(),
        entries.len()
    );
    if let Some(stage) = &journal.stage_id {
        println!("{} {}", "Stage:".dimmed(), stage);
    }
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
pub fn show(session_id: Option<String>) -> Result<()> {
    let work_dir = get_work_dir()?;

    let session = match session_id {
        Some(id) => id,
        None => detect_session_from_signals(&work_dir)?,
    };

    let journal = read_journal(&work_dir, &session)?;

    if journal.entries.is_empty() {
        println!(
            "{} No entries in memory journal for session '{}'",
            "ℹ".blue(),
            session
        );
        return Ok(());
    }

    println!("{}", "═".repeat(60));
    println!("{}", format!("Memory Journal: {session}").bold());
    if let Some(stage) = &journal.stage_id {
        println!("{} {}", "Stage:".dimmed(), stage);
    }
    println!("{} entries", journal.entries.len());
    println!("{}", "═".repeat(60));

    for entry in &journal.entries {
        println!("{}", format_entry_full(entry));
    }

    println!("\n{}", "═".repeat(60));

    Ok(())
}

/// List all memory journals
pub fn sessions() -> Result<()> {
    let work_dir = get_work_dir()?;
    let journals = list_journals(&work_dir)?;

    if journals.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }

    println!("{} Memory Journals ({})", "📚".bold(), journals.len());
    println!("{}", "─".repeat(60));

    for session_id in &journals {
        let journal = read_journal(&work_dir, session_id)?;
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

        println!(
            "{}",
            format_session_summary(
                session_id,
                journal.stage_id.as_deref(),
                journal.entries.len(),
                notes,
                decisions,
                questions
            )
        );
    }

    Ok(())
}

/// Promote memory entries to knowledge files
pub fn promote(entry_type: String, target: String, session_id: Option<String>) -> Result<()> {
    let work_dir = get_work_dir()?;
    let session = session_id
        .or_else(|| detect_session_from_signals(&work_dir).ok())
        .ok_or_else(|| anyhow::anyhow!("No session ID provided or detected. Use --session <id>"))?;

    // Parse entry type - "all" means promote all types
    let type_filter = if entry_type == "all" {
        None
    } else {
        Some(entry_type.parse::<MemoryEntryType>()?)
    };

    // Parse target knowledge file
    let target_file = match target.as_str() {
        "entry-points" => KnowledgeFile::EntryPoints,
        "patterns" => KnowledgeFile::Patterns,
        "conventions" => KnowledgeFile::Conventions,
        "mistakes" => KnowledgeFile::Mistakes,
        _ => anyhow::bail!(
            "Invalid target: {target}. Use: entry-points, patterns, conventions, mistakes"
        ),
    };

    // Get project root (go up from .work to find doc/loom/knowledge)
    let project_root = work_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Could not find project root"))?;

    let knowledge = KnowledgeDir::new(project_root);
    if !knowledge.exists() {
        anyhow::bail!("Knowledge directory does not exist. Run 'loom knowledge init' first.");
    }

    // Delete and retrieve the matching entries
    let deleted = delete_entries_by_type(&work_dir, &session, type_filter)?;

    if deleted.is_empty() {
        let type_desc = type_filter
            .map(|t| t.to_string())
            .unwrap_or_else(|| "any".to_string());
        println!(
            "{} No {} entries found in session '{}'",
            "ℹ".blue(),
            type_desc,
            session
        );
        return Ok(());
    }

    // Format entries for knowledge file
    let formatted = format_entries_for_knowledge(&deleted);

    // Append to knowledge file
    knowledge
        .append(target_file, &formatted)
        .context("Failed to append to knowledge file")?;

    // Print success message
    let type_desc = type_filter
        .map(|t| format!("{t} "))
        .unwrap_or_default();
    println!(
        "{} Promoted {} {}entries from session '{}' to {}",
        "✓".green(),
        deleted.len(),
        type_desc,
        session.cyan(),
        target_file.filename().cyan()
    );

    // Show promoted content preview
    for entry in deleted.iter().take(3) {
        let type_emoji = match entry.entry_type {
            MemoryEntryType::Note => "📝",
            MemoryEntryType::Decision => "✅",
            MemoryEntryType::Question => "❓",
        };
        println!(
            "  {} {}",
            type_emoji,
            truncate_for_display(&entry.content, 55)
        );
    }
    if deleted.len() > 3 {
        println!("  {} {} more...", "...".dimmed(), deleted.len() - 3);
    }

    Ok(())
}
