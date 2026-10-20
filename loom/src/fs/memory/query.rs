//! Query functions for memory journals.

use super::types::{MemoryEntry, MemoryEntryType, MemoryJournal};

/// Get recent entries from a journal (for recitation in signals)
pub fn get_recent_entries(journal: &MemoryJournal, max_entries: usize) -> Vec<&MemoryEntry> {
    let entries: Vec<_> = journal
        .entries
        .iter()
        .filter(|entry| entry.entry_type != MemoryEntryType::Receipt)
        .collect();
    let len = entries.len();
    if len <= max_entries {
        entries
    } else {
        entries[(len - max_entries)..].to_vec()
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

/// The entries of one type, in journal order.
pub(super) fn entries_of_type<'a>(
    entries: impl IntoIterator<Item = &'a MemoryEntry>,
    entry_type: MemoryEntryType,
) -> Vec<&'a MemoryEntry> {
    entries
        .into_iter()
        .filter(|entry| entry.entry_type == entry_type)
        .collect()
}
