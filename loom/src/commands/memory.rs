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

use crate::fs::memory::{
    append_entry, list_journals, query_entries, read_journal, validate_content, MemoryEntry,
    MemoryEntryType,
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

/// Try to detect the current session ID from the worktree
fn detect_session_id() -> Option<String> {
    // Check environment variable first (set by loom when spawning)
    if let Ok(session_id) = env::var("LOOM_SESSION_ID") {
        return Some(session_id);
    }

    // Try to detect from signal file in .work/signals/
    let cwd = env::current_dir().ok()?;
    let signals_dir = cwd.join(".work").join("signals");

    if !signals_dir.exists() {
        return None;
    }

    // Find most recent signal file
    let mut most_recent: Option<(String, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(&signals_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "md") {
            if let Some(stem) = path.file_stem() {
                let session_id = stem.to_string_lossy().to_string();
                let metadata = entry.metadata().ok()?;
                let modified = metadata.modified().ok()?;

                match &most_recent {
                    None => most_recent = Some((session_id, modified)),
                    Some((_, prev_time)) if modified > *prev_time => {
                        most_recent = Some((session_id, modified));
                    }
                    _ => {}
                }
            }
        }
    }

    most_recent.map(|(id, _)| id)
}

/// Record a note in the memory journal
pub fn note(text: String, session_id: Option<String>) -> Result<()> {
    validate_content(&text)?;

    let work_dir = get_work_dir()?;
    let session = session_id
        .or_else(detect_session_id)
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
        .or_else(detect_session_id)
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
        .or_else(detect_session_id)
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
        None => detect_session_id().ok_or_else(|| {
            anyhow::anyhow!("No session ID provided or detected. Use --session <id>")
        })?,
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
        None => detect_session_id().ok_or_else(|| {
            anyhow::anyhow!("No session ID provided or detected. Use --session <id>")
        })?,
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

/// Truncate a string for display
fn truncate_for_display(s: &str, max_len: usize) -> String {
    let single_line: String = s.lines().collect::<Vec<_>>().join(" ");

    if single_line.len() <= max_len {
        single_line
    } else {
        format!("{}…", &single_line[..max_len - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_for_display() {
        assert_eq!(truncate_for_display("short", 10), "short");
        assert_eq!(
            truncate_for_display("this is a longer string", 10),
            "this is a…"
        );
        assert_eq!(
            truncate_for_display("line1\nline2\nline3", 20),
            "line1 line2 line3"
        );
    }
}
