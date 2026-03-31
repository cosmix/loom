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

/// Aliases that map to canonical knowledge file names.
const KNOWLEDGE_ALIASES: &[(&str, &str)] = &[
    ("deps", "stack"),
    ("tech", "stack"),
    ("debt", "concerns"),
    ("issues", "concerns"),
];

/// Complete knowledge file names for `loom knowledge show/update`
///
/// Includes aliases (deps/tech -> stack, debt/issues -> concerns).
///
/// # Arguments
///
/// * `prefix` - Partial file name prefix to filter results
///
/// # Returns
///
/// List of matching knowledge file names (canonical + matching aliases)
pub fn complete_knowledge_files(prefix: &str) -> Result<Vec<String>> {
    let mut results: Vec<String> = KNOWLEDGE_FILES
        .iter()
        .filter(|name| prefix.is_empty() || name.starts_with(prefix))
        .map(|s| s.to_string())
        .collect();

    // Add matching aliases
    for (alias, _canonical) in KNOWLEDGE_ALIASES {
        if prefix.is_empty() || alias.starts_with(prefix) {
            results.push(alias.to_string());
        }
    }

    results.sort();
    Ok(results)
}
