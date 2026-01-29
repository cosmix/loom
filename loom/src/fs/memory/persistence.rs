//! Persistence operations for memory journals (deletion, listing, archiving).

use super::constants::MEMORY_HEADER;
use super::parser::format_entry;
use super::storage::{memory_dir, memory_file_path, read_journal};
use super::types::{MemoryEntry, MemoryEntryType};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

/// Delete entries by type from a session's memory journal
/// Returns the deleted entries for promotion to knowledge
pub fn delete_entries_by_type(
    work_dir: &Path,
    session_id: &str,
    entry_type: Option<MemoryEntryType>,
) -> Result<Vec<MemoryEntry>> {
    let file_path = memory_file_path(work_dir, session_id);

    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let journal = read_journal(work_dir, session_id)?;
    if journal.entries.is_empty() {
        return Ok(Vec::new());
    }

    // Partition entries into those to delete and those to keep
    let (to_delete, to_keep): (Vec<_>, Vec<_>) = journal
        .entries
        .into_iter()
        .partition(|e| entry_type.is_none_or(|t| e.entry_type == t));

    if to_delete.is_empty() {
        return Ok(Vec::new());
    }

    // Rewrite the journal with only the kept entries
    let stage_line = journal
        .stage_id
        .as_ref()
        .map(|s| format!("**Stage**: {s}\n"))
        .unwrap_or_default();

    let header = format!(
        "{MEMORY_HEADER}# Memory Journal: {session_id}\n\n**Session**: {session_id}\n{stage_line}**Created**: {}\n\n---\n\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );

    let mut content = header;
    for entry in &to_keep {
        content.push_str(&format_entry(entry));
    }

    fs::write(&file_path, content)
        .with_context(|| format!("Failed to rewrite memory journal: {}", file_path.display()))?;

    Ok(to_delete)
}

/// List all memory journals in the work directory
pub fn list_journals(work_dir: &Path) -> Result<Vec<String>> {
    let memory_path = memory_dir(work_dir);

    if !memory_path.exists() {
        return Ok(Vec::new());
    }

    let mut journals = Vec::new();

    for entry in fs::read_dir(&memory_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "md") {
            if let Some(stem) = path.file_stem() {
                journals.push(stem.to_string_lossy().to_string());
            }
        }
    }

    Ok(journals)
}

/// Copy memory journal to crash recovery location
pub fn preserve_for_crash(work_dir: &Path, session_id: &str) -> Result<Option<PathBuf>> {
    let source = memory_file_path(work_dir, session_id);

    if !source.exists() {
        return Ok(None);
    }

    let crashes_dir = work_dir.join("crashes");
    if !crashes_dir.exists() {
        fs::create_dir_all(&crashes_dir)?;
    }

    let dest = crashes_dir.join(format!("memory-{session_id}.md"));
    fs::copy(&source, &dest)
        .with_context(|| format!("Failed to preserve memory for crash: {}", source.display()))?;

    Ok(Some(dest))
}

/// Extract key notes from memory for review on completion
pub fn extract_key_notes(journal: &super::types::MemoryJournal) -> Vec<String> {
    let mut key_notes = Vec::new();

    // Extract all decisions as key notes
    for entry in &journal.entries {
        if entry.entry_type == MemoryEntryType::Decision {
            let note = if let Some(ctx) = &entry.context {
                format!("{} ({})", entry.content, ctx)
            } else {
                entry.content.clone()
            };
            key_notes.push(note);
        }
    }

    key_notes
}

/// Validate memory entry content
pub fn validate_content(content: &str) -> Result<()> {
    if content.is_empty() {
        bail!("Memory entry content cannot be empty");
    }

    if content.len() > 2000 {
        bail!(
            "Memory entry content too long: {} characters (max 2000)",
            content.len()
        );
    }

    Ok(())
}
