//! Knowledge file completions for shell tab-completion.

use anyhow::Result;

/// Valid knowledge file names
const KNOWLEDGE_FILES: &[&str] = &[
    "architecture",
    "entry-points",
    "patterns",
    "conventions",
    "mistakes",
    "stack",
    "concerns",
];

/// Complete knowledge file names for `loom knowledge show/update`
///
/// # Arguments
///
/// * `prefix` - Partial file name prefix to filter results
///
/// # Returns
///
/// List of matching knowledge file names
pub fn complete_knowledge_files(prefix: &str) -> Result<Vec<String>> {
    let results: Vec<String> = KNOWLEDGE_FILES
        .iter()
        .filter(|name| prefix.is_empty() || name.starts_with(prefix))
        .map(|s| s.to_string())
        .collect();

    Ok(results)
}
