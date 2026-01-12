//! Hooks settings generator for Claude Code worktrees.
//!
//! This module generates the `.claude/settings.json` configuration
//! that sets up hooks for loom-managed Claude Code sessions.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use super::config::HooksConfig;

/// Generate settings.json content with hooks configuration
///
/// This creates or updates the settings.json with hook definitions
/// while preserving existing settings from the main repo.
pub fn generate_hooks_settings(
    config: &HooksConfig,
    existing_settings: Option<&Value>,
) -> Result<Value> {
    // Start with existing settings or empty object
    let mut settings = existing_settings.cloned().unwrap_or_else(|| json!({}));

    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings must be a JSON object"))?;

    // Set trust dialog accepted
    obj.insert("hasTrustDialogAccepted".to_string(), json!(true));

    // Ensure permissions object exists and set acceptEdits
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    permissions.insert("defaultMode".to_string(), json!("acceptEdits"));

    // Generate hooks configuration
    let hooks = config.to_settings_hooks();
    let hooks_json: Vec<Value> = hooks
        .iter()
        .map(|h| {
            json!({
                "matcher": h.matcher,
                "hooks": {
                    "preToolUse": h.hooks.pre_tool_use,
                    "postToolUse": h.hooks.post_tool_use
                }
            })
        })
        .collect();

    obj.insert("hooks".to_string(), json!(hooks_json));

    // Add environment variables for hooks to access
    let env = obj
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("env must be a JSON object"))?;

    env.insert("LOOM_STAGE_ID".to_string(), json!(config.stage_id));
    env.insert("LOOM_SESSION_ID".to_string(), json!(config.session_id));
    env.insert(
        "LOOM_WORK_DIR".to_string(),
        json!(config.work_dir.display().to_string()),
    );

    Ok(settings)
}

/// Set up hooks for a worktree by creating/updating settings.json
///
/// This function:
/// 1. Reads existing settings.json if present
/// 2. Generates hooks configuration
/// 3. Writes the merged settings to the worktree's .claude/settings.json
pub fn setup_hooks_for_worktree(
    worktree_path: &Path,
    config: &HooksConfig,
) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    // Ensure .claude directory exists
    if !claude_dir.exists() {
        std::fs::create_dir_all(&claude_dir)
            .with_context(|| format!("Failed to create .claude directory: {}", claude_dir.display()))?;
    }

    // Read existing settings if present
    let existing: Option<Value> = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read settings: {}", settings_path.display()))?;
        serde_json::from_str(&content).ok()
    } else {
        None
    };

    // Generate merged settings with hooks
    let settings = generate_hooks_settings(config, existing.as_ref())?;

    // Write settings
    let content = serde_json::to_string_pretty(&settings)
        .with_context(|| "Failed to serialize settings")?;
    std::fs::write(&settings_path, content)
        .with_context(|| format!("Failed to write settings: {}", settings_path.display()))?;

    Ok(())
}

/// Find the loom hooks directory
///
/// Looks for hooks in:
/// 1. `$LOOM_HOOKS_DIR` environment variable
/// 2. Relative to loom binary: `../hooks/` or `./hooks/`
/// 3. Standard installation paths
pub fn find_hooks_dir() -> Option<std::path::PathBuf> {
    // Check environment variable first
    if let Ok(dir) = std::env::var("LOOM_HOOKS_DIR") {
        let path = std::path::PathBuf::from(dir);
        if path.exists() {
            return Some(path);
        }
    }

    // Try relative to current executable
    if let Ok(exe_path) = std::env::current_exe() {
        // Try sibling hooks/ directory (development layout)
        if let Some(parent) = exe_path.parent() {
            let hooks_dir = parent.join("hooks");
            if hooks_dir.exists() {
                return Some(hooks_dir);
            }

            // Try ../hooks/ (installed layout)
            if let Some(grandparent) = parent.parent() {
                let hooks_dir = grandparent.join("hooks");
                if hooks_dir.exists() {
                    return Some(hooks_dir);
                }
            }
        }
    }

    // Try relative to current directory (for development)
    let cwd_hooks = std::path::PathBuf::from("hooks");
    if cwd_hooks.exists() {
        return Some(cwd_hooks);
    }

    // Try loom/hooks from project root
    let loom_hooks = std::path::PathBuf::from("loom/hooks");
    if loom_hooks.exists() {
        return Some(loom_hooks);
    }

    None
}
