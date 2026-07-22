//! Hooks settings generator for Claude Code worktrees.
//!
//! This module generates the `.claude/settings.local.json` configuration
//! that sets up hooks for loom-managed Claude Code sessions.
//! Hooks are stored in settings.local.json (not settings.json) because
//! they contain user-specific paths (e.g., ~/.claude/hooks/loom/).

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use super::config::HooksConfig;
use crate::fs::permissions::configure_loom_hooks;

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

    // Write the resolved permission mode into permissions.defaultMode using
    // the camelCase mapping owned by `PermissionMode::as_settings_value`.
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;
    permissions.insert(
        "defaultMode".to_string(),
        json!(config.permission_mode.as_settings_value()),
    );

    // Ensure global hooks (commit-filter, git-add-guard, worktree-isolation, etc.) are
    // present, referencing the host hook scripts at ~/.claude/hooks/loom/.
    configure_loom_hooks(obj)?;

    // Generate session-specific hooks
    let session_hooks = config.to_settings_hooks();

    // Merge session hooks with existing (global hooks are already in place)
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

    // Add environment variables for hooks to access.
    //
    // LOOM_WORK_DIR is the only loom var persisted here: it is stable for the
    // lifetime of the repo, so it can never go stale.
    let env = obj
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("env must be a JSON object"))?;

    env.insert(
        "LOOM_WORK_DIR".to_string(),
        json!(config.work_dir.display().to_string()),
    );

    // IMPORTANT: Never persist per-session identity (LOOM_MAIN_AGENT_PID,
    // LOOM_STAGE_ID, LOOM_SESSION_ID) in settings files. These are exported
    // by the session wrapper script so they always match the running process;
    // a settings `env` block overrides the process environment, so a persisted
    // value from an earlier session would shadow the wrapper's fresh exports
    // (wrong-stage `loom memory` entries, heartbeats for dead sessions,
    // commit-filter misidentifying the main agent). Also scrub any stale
    // values written by older loom versions.
    crate::fs::permissions::scrub_session_identity_env(&mut settings);

    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings must be a JSON object"))?;

    // Add narrow Read permissions for resolved absolute paths of shared state.
    // In worktrees, .work/ is a symlink to ../../.work. Claude Code resolves symlinks
    // before permission matching, so relative patterns like Read(.work/**) fail because
    // the resolved path is outside the worktree's project root. These absolute-path
    // permissions cover only the specific files this session needs.
    //
    // IMPORTANT: Claude Code requires the // prefix for absolute filesystem paths.
    // A single / means "relative to project root", NOT absolute. See:
    // https://code.claude.com/docs/en/permissions.md
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    let allow = permissions.entry("allow").or_insert_with(|| json!([]));
    if let Some(allow_arr) = allow.as_array_mut() {
        // Signal files (this session's signal + any recovery signals)
        // Use / prefix on absolute paths for Claude Code's // convention
        let signals_dir = config.work_dir.join("signals");
        allow_arr.push(json!(format!("Read(/{}/**)", signals_dir.display())));

        // Shared config (plan reference, base branch)
        let config_toml = config.work_dir.join("config.toml");
        allow_arr.push(json!(format!("Read(/{})", config_toml.display())));

        // Handoff files (context continuations)
        let handoffs_dir = config.work_dir.join("handoffs");
        allow_arr.push(json!(format!("Read(/{}/**)", handoffs_dir.display())));

        // Plan files in the main project (doc/plans/ contains only plan markdown)
        if let Some(project_root) = config.work_dir.parent() {
            let plans_dir = project_root.join("doc").join("plans");
            allow_arr.push(json!(format!("Read(/{}/**)", plans_dir.display())));
        }
    }

    Ok(settings)
}

/// Set up hooks for a worktree by creating/updating settings.local.json
///
/// This function:
/// 1. Reads existing settings.local.json if present (containing global hooks + sandbox)
/// 2. Generates session-specific hooks configuration
/// 3. Merges session hooks with global hooks
/// 4. Writes the merged settings to the worktree's .claude/settings.local.json
pub fn setup_hooks_for_worktree(worktree_path: &Path, config: &HooksConfig) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::config::HooksConfig;
    use crate::plan::schema::PermissionMode;
    use tempfile::TempDir;

    #[test]
    fn test_generate_hooks_settings_includes_global_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let config = HooksConfig::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            PermissionMode::Default,
        );

        let settings = generate_hooks_settings(&config, None).unwrap();

        let pre_tool_use = settings["hooks"]["PreToolUse"]
            .as_array()
            .expect("PreToolUse must be an array");

        let commands: Vec<String> = pre_tool_use
            .iter()
            .flat_map(|entry| {
                entry["hooks"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|hook| hook["command"].as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .collect();

        assert!(
            commands.iter().any(|c| c.contains("commit-filter.sh")),
            "commit-filter.sh should be a registered global hook, got: {commands:?}"
        );
        assert!(
            commands
                .iter()
                .any(|c| c.contains("prefer-modern-tools.sh")),
            "prefer-modern-tools.sh should be a registered global hook, got: {commands:?}"
        );

        let stop_hooks = settings["hooks"]["Stop"]
            .as_array()
            .expect("Stop must be an array");
        let stop_commands: Vec<String> = stop_hooks
            .iter()
            .flat_map(|entry| {
                entry["hooks"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|hook| hook["command"].as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            stop_commands.iter().any(|c| c.contains("commit-guard.sh")),
            "commit-guard.sh should be in Stop, got: {stop_commands:?}"
        );
    }
}
