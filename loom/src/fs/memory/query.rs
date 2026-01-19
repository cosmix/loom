//! Query and summarization functions for memory journals.

use super::export::truncate_content;
use super::types::{MemoryEntry, MemoryEntryType, MemoryJournal};

/// Get recent entries from a journal (for recitation in signals)
pub fn get_recent_entries(journal: &MemoryJournal, max_entries: usize) -> Vec<&MemoryEntry> {
    let len = journal.entries.len();
    if len <= max_entries {
        journal.entries.iter().collect()
    } else {
        journal.entries[(len - max_entries)..].iter().collect()
    }
}

/// Query memory entries by search term
pub fn query_entries<'a>(journal: &'a MemoryJournal, search: &str) -> Vec<&'a MemoryEntry> {
    let search_lower = search.to_lowercase();
    journal
        .entries
        .iter()
        .filter(|e| {
            e.content.to_lowercase().contains(&search_lower)
                || e.context
                    .as_ref()
                    .is_some_and(|c| c.to_lowercase().contains(&search_lower))
        })
        .collect()
}

/// Generate a summary of the memory journal (for context threshold)
pub fn generate_summary(journal: &MemoryJournal, max_entries: usize) -> String {
    let mut summary = String::new();
    summary.push_str("## Summary\n\n");
    summary.push_str("Auto-generated summary at context threshold.\n\n");

    let notes: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Note)
        .collect();
    let decisions: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Decision)
        .collect();
    let questions: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Question)
        .collect();

    summary.push_str(&format!("- **Total entries**: {}\n", journal.entries.len()));
    summary.push_str(&format!("- **Notes**: {}\n", notes.len()));
    summary.push_str(&format!("- **Decisions**: {}\n", decisions.len()));
    summary.push_str(&format!("- **Questions**: {}\n\n", questions.len()));

    // Key decisions (most recent)
    if !decisions.is_empty() {
        summary.push_str("### Key Decisions\n\n");
        for entry in decisions.iter().rev().take(max_entries) {
            summary.push_str(&format!(
                "- {}\n",
                truncate_content(&entry.content, 200)
            ));
        }
        summary.push('\n');
    }

    // Open questions (all)
    if !questions.is_empty() {
        summary.push_str("### Open Questions\n\n");
        for entry in &questions {
            summary.push_str(&format!(
                "- {}\n",
                truncate_content(&entry.content, 200)
            ));
        }
        summary.push('\n');
    }

    summary
}
