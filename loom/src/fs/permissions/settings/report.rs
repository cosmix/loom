//! Persist-and-report tail for merged `.claude/settings.local.json` changes.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

use crate::fs::locking::locked_write;

/// Write `path` if any `(changed, label)` pair in `changes` is set, and print
/// each changed label under `verbose`; prints a single "already configured"
/// line instead when nothing changed.
///
/// Factored out of [`super::ensure_loom_hooks_local_inner`], which builds
/// `changes` from its own merge steps (hooks, env, worktree isolation, codex
/// allowances, stale-env scrubbing, read-deny healing) and hands the finished
/// array here to persist and report.
pub(super) fn write_settings_local_if_changed(
    settings: &Value,
    path: &Path,
    changes: &[(bool, &str)],
    verbose: bool,
) -> Result<()> {
    if changes.iter().any(|(changed, _)| *changed) {
        let content = serde_json::to_string_pretty(settings)
            .context("Failed to serialize settings.local.json to JSON")?;

        locked_write(path, &content)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        if verbose {
            for (_, change) in changes.iter().filter(|(changed, _)| *changed) {
                println!("  {change} in .claude/settings.local.json");
            }
        }
    } else if verbose {
        println!("  Hooks and env vars already configured in .claude/settings.local.json");
    }

    Ok(())
}
