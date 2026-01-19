//! Memory command implementations for managing session memory journals.
//!
//! Commands:
//! - `loom memory note <text>` - Record a note
//! - `loom memory decision <text> [--context <ctx>]` - Record a decision
//! - `loom memory question <text>` - Record a question
//! - `loom memory query <search>` - Search memory entries
//! - `loom memory list [--session <id>]` - List memory entries
//! - `loom memory show [--session <id>]` - Show full memory journal

use anyhow::{Context, Result};
use colored::Colorize;
use std::env;

use crate::commands::common::{detect_session_from_signals, truncate_for_display};
use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::fs::memory::{
    append_entry, delete_entries_by_type, list_journals, query_entries, read_journal,
    validate_content, MemoryEntry, MemoryEntryType,
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

    println!(
        "{} Recorded note in session '{}'",
        "📝".green(),
        session.cyan()
    );
    println!("  {}", truncate_for_display(&text, 60));

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

    println!(
        "{} Recorded decision in session '{}'",
        "✅".green(),
        session.cyan()
    );
    println!("  {}", truncate_for_display(&text, 60));

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

    println!(
        "{} Recorded question in session '{}'",
        "❓".green(),
        session.cyan()
    );
    println!("  {}", truncate_for_display(&text, 60));

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
            let time = entry.timestamp.format("%H:%M:%S").to_string();
            let type_emoji = match entry.entry_type {
                MemoryEntryType::Note => "📝",
                MemoryEntryType::Decision => "✅",
                MemoryEntryType::Question => "❓",
            };

            println!(
                "{} {} {} {}",
                time.dimmed(),
                type_emoji,
                entry.entry_type.display_name().cyan(),
                truncate_for_display(&entry.content, 50)
            );

            if let Some(ctx) = &entry.context {
                println!(
                    "  {} {}",
                    "→".dimmed(),
                    truncate_for_display(ctx, 48).yellow()
                );
            }
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
        let time = entry.timestamp.format("%H:%M:%S").to_string();
        let type_emoji = match entry.entry_type {
            MemoryEntryType::Note => "📝",
            MemoryEntryType::Decision => "✅",
            MemoryEntryType::Question => "❓",
        };

        println!(
            "{} {} {} {}",
            time.dimmed(),
            type_emoji,
            entry.entry_type.display_name().cyan(),
            truncate_for_display(&entry.content, 50)
        );

        if let Some(ctx) = &entry.context {
            println!(
                "  {} {}",
                "→".dimmed(),
                truncate_for_display(ctx, 48).yellow()
            );
        }
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
        let time = entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let type_emoji = match entry.entry_type {
            MemoryEntryType::Note => "📝",
            MemoryEntryType::Decision => "✅",
            MemoryEntryType::Question => "❓",
        };

        println!(
            "\n{} {} {}",
            type_emoji,
            entry.entry_type.display_name().bold(),
            time.dimmed()
        );
        println!("{}", "─".repeat(40));
        println!("{}", entry.content);

        if let Some(ctx) = &entry.context {
            println!("\n{} {}", "Context:".cyan(), ctx);
        }
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

        let stage_info = journal
            .stage_id
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();

        println!(
            "{}{} - {} entries (📝 {} / ✅ {} / ❓ {})",
            session_id.cyan(),
            stage_info.dimmed(),
            journal.entries.len(),
            notes,
            decisions,
            questions
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

/// Format memory entries for inclusion in a knowledge file
fn format_entries_for_knowledge(entries: &[MemoryEntry]) -> String {
    let mut output = String::new();

    // Group by type
    let notes: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Note)
        .collect();
    let decisions: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Decision)
        .collect();
    let questions: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Question)
        .collect();

    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M");
    output.push_str(&format!("## Promoted from Memory [{timestamp}]\n\n"));

    // Format notes as bullet list
    if !notes.is_empty() {
        output.push_str("### Notes\n\n");
        for entry in &notes {
            output.push_str(&format!("- {}\n", entry.content));
        }
        output.push('\n');
    }

    // Format decisions with rationale
    if !decisions.is_empty() {
        output.push_str("### Decisions\n\n");
        for entry in &decisions {
            output.push_str(&format!("- **{}**", entry.content));
            if let Some(ctx) = &entry.context {
                output.push_str(&format!("\n  - *Rationale:* {ctx}"));
            }
            output.push('\n');
        }
        output.push('\n');
    }

    // Format questions as bullet list
    if !questions.is_empty() {
        output.push_str("### Questions\n\n");
        for entry in &questions {
            output.push_str(&format!("- {}\n", entry.content));
        }
        output.push('\n');
    }

    output
}
