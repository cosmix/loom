//! Trust management for loom worktrees

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Add worktrees directory to Claude Code's global trusted directories
///
/// This modifies `~/.claude.json` to include the repo's `.worktrees/` path
/// in `trustedDirectories`, preventing the "trust this folder?" prompt
/// when spawning sessions in worktrees.
pub fn add_worktrees_to_global_trust(repo_root: &Path) -> Result<()> {
    let home_dir = dirs::home_dir().context("Failed to determine home directory")?;
    let global_settings_path = home_dir.join(".claude.json");

    // Compute canonical path to worktrees directory
    let worktrees_dir = repo_root.join(".worktrees");
    let worktrees_path = worktrees_dir
        .canonicalize()
        .unwrap_or_else(|_| worktrees_dir.clone())
        .to_string_lossy()
        .to_string();

    // Load existing settings or create new
    let mut settings: Value = if global_settings_path.exists() {
        let content = fs::read_to_string(&global_settings_path)
            .with_context(|| format!("Failed to read {}", global_settings_path.display()))?;

        serde_json::from_str(&content).with_context(|| {
            format!("Failed to parse {} as JSON", global_settings_path.display())
        })?
    } else {
        json!({})
    };

    // Ensure settings is an object
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("~/.claude.json must be a JSON object"))?;

    // Get or create trustedDirectories array
    let trusted_dirs = settings_obj
        .entry("trustedDirectories")
        .or_insert_with(|| json!([]));

    let trusted_arr = trusted_dirs
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("trustedDirectories must be a JSON array"))?;

    // Check if path is already trusted (either exact match or parent is trusted)
    let already_trusted = trusted_arr.iter().any(|v| {
        v.as_str()
            .map(|s| worktrees_path.starts_with(s) || s == worktrees_path)
            .unwrap_or(false)
    });

    if already_trusted {
        return Ok(());
    }

    // Add worktrees path to trusted directories
    trusted_arr.push(json!(worktrees_path));

    // Write back
    let content = serde_json::to_string_pretty(&settings)
        .context("Failed to serialize global settings to JSON")?;

    fs::write(&global_settings_path, content)
        .with_context(|| format!("Failed to write {}", global_settings_path.display()))?;

    println!("  Added {worktrees_path} to trusted directories");

    Ok(())
}
