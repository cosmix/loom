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
/// This merges session-specific hooks into existing settings.json while
/// preserving global hooks from the main repo. Session hooks are appended
/// to each event type's hook array, with duplicate detection to prevent
/// hook duplication.
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

    // Generate session-specific hooks
    let session_hooks = config.to_settings_hooks();

    // Merge with existing hooks instead of replacing
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks must be a JSON object"))?;

    // Merge each event type's hooks
    for (event_type, session_rules) in session_hooks {
        let event_hooks = hooks
            .entry(&event_type)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks.{event_type} must be an array"))?;

        // Convert session rules to JSON and append to existing hooks
        for rule in session_rules {
            let rule_json = serde_json::to_value(&rule)
                .with_context(|| format!("Failed to serialize hook rule for {event_type}"))?;

            // Check for duplicates before adding
            if !event_hooks.iter().any(|existing| existing == &rule_json) {
                event_hooks.push(rule_json);
            }
        }
    }

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
/// 1. Reads existing settings.json if present (containing global hooks)
/// 2. Generates session-specific hooks configuration
/// 3. Merges session hooks with global hooks
/// 4. Writes the merged settings to the worktree's .claude/settings.json
pub fn setup_hooks_for_worktree(worktree_path: &Path, config: &HooksConfig) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    // Ensure .claude directory exists
    if !claude_dir.exists() {
        std::fs::create_dir_all(&claude_dir).with_context(|| {
            format!(
                "Failed to create .claude directory: {}",
                claude_dir.display()
            )
        })?;
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
    let content =
        serde_json::to_string_pretty(&settings).with_context(|| "Failed to serialize settings")?;
    std::fs::write(&settings_path, content)
        .with_context(|| format!("Failed to write settings: {}", settings_path.display()))?;

    Ok(())
}

/// Find the loom hooks directory
///
/// Looks for hooks in:
/// 1. `$LOOM_HOOKS_DIR` environment variable (for testing/override)
/// 2. `~/.claude/hooks/loom/` (standard installation location)
///
/// Returns None if hooks are not installed. Run `loom init` to install hooks.
pub fn find_hooks_dir() -> Option<std::path::PathBuf> {
    // Check environment variable first (for testing/override)
    if let Ok(dir) = std::env::var("LOOM_HOOKS_DIR") {
        let path = std::path::PathBuf::from(dir);
        if path.exists() {
            return Some(path);
        }
    }

    // Check standard installation location: ~/.claude/hooks/loom/
    if let Some(home_dir) = dirs::home_dir() {
        let installed_hooks = home_dir.join(".claude/hooks/loom");
        if installed_hooks.exists() {
            return Some(installed_hooks);
        }
    }

    None
}
