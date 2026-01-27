//! Memory entry type completions for shell tab-completion.

use anyhow::Result;

/// Valid memory entry types
const MEMORY_ENTRY_TYPES: &[&str] = &["note", "decision", "question"];

/// Valid memory promote entry types (includes "all")
const MEMORY_PROMOTE_ENTRY_TYPES: &[&str] = &["note", "decision", "question", "all"];

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
    let results: Vec<String> = MEMORY_ENTRY_TYPES
        .iter()
        .filter(|name| prefix.is_empty() || name.starts_with(prefix))
        .map(|s| s.to_string())
        .collect();

    Ok(results)
}

/// Complete memory promote entry types for `loom memory promote`
///
/// # Arguments
///
/// * `prefix` - Partial entry type prefix to filter results
///
/// # Returns
///
/// List of matching memory promote entry types
pub fn complete_memory_promote_entry_types(prefix: &str) -> Result<Vec<String>> {
    let results: Vec<String> = MEMORY_PROMOTE_ENTRY_TYPES
        .iter()
        .filter(|name| prefix.is_empty() || name.starts_with(prefix))
        .map(|s| s.to_string())
        .collect();

    Ok(results)
}
