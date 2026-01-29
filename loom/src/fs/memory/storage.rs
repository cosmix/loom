//! File storage operations for memory journals.

use super::constants::MEMORY_HEADER;
use super::parser::{format_entry, parse_journal};
use super::types::{MemoryEntry, MemoryJournal};
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Get the memory directory path
pub fn memory_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("memory")
}

/// Get the path to a session's memory file
pub fn memory_file_path(work_dir: &Path, session_id: &str) -> PathBuf {
    memory_dir(work_dir).join(format!("{session_id}.md"))
}

/// Initialize the memory directory
pub fn init_memory_dir(work_dir: &Path) -> Result<()> {
    let memory_path = memory_dir(work_dir);

    if !memory_path.exists() {
        fs::create_dir_all(&memory_path).with_context(|| {
            format!(
                "Failed to create memory directory: {}",
                memory_path.display()
            )
        })?;
    }

    Ok(())
}

/// Create a new memory journal for a session
pub fn create_journal(
    work_dir: &Path,
    session_id: &str,
    stage_id: Option<&str>,
) -> Result<PathBuf> {
    init_memory_dir(work_dir)?;

    let file_path = memory_file_path(work_dir, session_id);
    let stage_line = stage_id
        .map(|s| format!("**Stage**: {s}\n"))
        .unwrap_or_default();

    let header = format!(
        "{MEMORY_HEADER}# Memory Journal: {session_id}\n\n**Session**: {session_id}\n{stage_line}**Created**: {}\n\n---\n\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );

    fs::write(&file_path, header)
        .with_context(|| format!("Failed to create memory journal: {}", file_path.display()))?;

    Ok(file_path)
}

/// Append an entry to a session's memory journal
pub fn append_entry(work_dir: &Path, session_id: &str, entry: &MemoryEntry) -> Result<()> {
    let file_path = memory_file_path(work_dir, session_id);

    // Create journal if it doesn't exist
    if !file_path.exists() {
        create_journal(work_dir, session_id, None)?;
    }

    let formatted = format_entry(entry);

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&file_path)
        .with_context(|| format!("Failed to open memory journal: {}", file_path.display()))?;

    file.write_all(formatted.as_bytes()).with_context(|| {
        format!(
            "Failed to append to memory journal: {}",
            file_path.display()
        )
    })?;

    Ok(())
}

/// Read a session's memory journal
pub fn read_journal(work_dir: &Path, session_id: &str) -> Result<MemoryJournal> {
    let file_path = memory_file_path(work_dir, session_id);

    if !file_path.exists() {
        return Ok(MemoryJournal {
            session_id: session_id.to_string(),
            ..Default::default()
        });
    }

    let content = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read memory journal: {}", file_path.display()))?;

    parse_journal(&content, session_id)
}

/// Write summary to the journal file
pub fn write_summary(work_dir: &Path, session_id: &str, summary: &str) -> Result<()> {
    let file_path = memory_file_path(work_dir, session_id);

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&file_path)
        .with_context(|| format!("Failed to open memory journal: {}", file_path.display()))?;

    file.write_all(summary.as_bytes()).with_context(|| {
        format!(
            "Failed to write summary to memory journal: {}",
            file_path.display()
        )
    })?;

    Ok(())
}
