//! Checkpoint status completions for shell tab-completion.

use anyhow::Result;

/// Valid checkpoint statuses
const CHECKPOINT_STATUSES: &[&str] = &["completed", "blocked", "needs_help"];

/// Complete checkpoint statuses for `loom checkpoint create --status`
///
/// # Arguments
///
/// * `prefix` - Partial status prefix to filter results
///
/// # Returns
///
/// List of matching checkpoint statuses
pub fn complete_checkpoint_statuses(prefix: &str) -> Result<Vec<String>> {
    let results: Vec<String> = CHECKPOINT_STATUSES
        .iter()
        .filter(|name| prefix.is_empty() || name.starts_with(prefix))
        .map(|s| s.to_string())
        .collect();

    Ok(results)
}
