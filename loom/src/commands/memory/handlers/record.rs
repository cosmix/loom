//! Recording handlers: `note`, `decision`, `change`, `question`.

use anyhow::Result;

use crate::fs::memory::{append_entry, validate_content, MemoryEntry, MemoryEntryType};

use super::super::formatters::format_record_success;
use super::work_dir::{get_or_create_work_dir, validate_stage_id, AD_HOC_STAGE_ID};

/// Shared implementation behind `note`, `decision`, `change`, and `question`.
fn record(
    entry_type: MemoryEntryType,
    text: String,
    context: Option<String>,
    stage_id: Option<String>,
) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref ctx) = context {
        validate_content(ctx)?;
    }
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_or_create_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .unwrap_or_else(|| AD_HOC_STAGE_ID.to_string());

    let entry = match context {
        Some(ctx) => MemoryEntry::with_context(entry_type, text.clone(), ctx),
        None => MemoryEntry::new(entry_type, text.clone()),
    };
    append_entry(&work_dir, &stage, &entry)?;

    println!("{}", format_record_success(&entry_type, &stage, &text));

    Ok(())
}

/// Record a note in the memory journal
pub fn note(text: String, stage_id: Option<String>) -> Result<()> {
    record(MemoryEntryType::Note, text, None, stage_id)
}

/// Record a decision in the memory journal
pub fn decision(text: String, context: Option<String>, stage_id: Option<String>) -> Result<()> {
    record(MemoryEntryType::Decision, text, context, stage_id)
}

/// Record a file change in the memory journal
pub fn change(text: String, stage_id: Option<String>) -> Result<()> {
    record(MemoryEntryType::Change, text, None, stage_id)
}

/// Record a question in the memory journal
pub fn question(text: String, stage_id: Option<String>) -> Result<()> {
    record(MemoryEntryType::Question, text, None, stage_id)
}
