//! Memory entry type completions for shell tab-completion.

use anyhow::Result;

/// Complete memory entry types for `loom memory list --entry-type`
///
/// # Arguments
///
/// * `prefix` - Partial entry type prefix to filter results
///
/// # Returns
///
/// List of matching memory entry types
pub fn complete_memory_entry_types(prefix: &str) -> Result<Vec<String>> {
    let results: Vec<String> = crate::fs::memory::MemoryEntryType::all()
        .iter()
        .map(ToString::to_string)
        .filter(|name| prefix.is_empty() || name.starts_with(prefix))
        .collect();

    Ok(results)
}
