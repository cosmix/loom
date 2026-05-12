//! Hooks settings generator for Claude Code worktrees.
//!
//! This module generates the `.claude/settings.local.json` configuration
//! that sets up hooks for loom-managed Claude Code sessions.
//! Hooks are stored in settings.local.json (not settings.json) because
//! they contain user-specific paths (e.g., ~/.claude/hooks/loom/).

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::config::HooksConfig;
use crate::fs::permissions::{configure_loom_hooks, configure_loom_hooks_for_container};
use crate::sandbox::scrub_settings_env_for_backend;

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

    // Scrub sensitive env keys from any inherited env block if we're targeting
    // a container backend. Native hosts already see the host env directly, so
    // there's no copy step to scrub there.
    scrub_settings_env_for_backend(&mut settings, config.backend);

    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings must be a JSON object"))?;

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
    // present. For native sessions these use host paths; for container sessions the hook
    // scripts are bind-mounted at /home/loom/.claude/hooks/loom/ so we emit that stable
    // path instead.  Without this branch a container agent would reference host paths
    // that are unreachable inside the container.
    match config.backend {
        crate::plan::schema::BackendType::Native => configure_loom_hooks(obj)?,
        crate::plan::schema::BackendType::Container => configure_loom_hooks_for_container(obj)?,
    };

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

    // Add environment variables for hooks to access
    let env = obj
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("env must be a JSON object"))?;

    env.insert("LOOM_STAGE_ID".to_string(), json!(config.stage_id));
    env.insert("LOOM_SESSION_ID".to_string(), json!(config.session_id));
    // For container sessions, write the in-container path. The host
    // .work directory is bind-mounted at /repo/.work, so hooks running
    // inside the container need that path; the host path is unreachable.
    let work_dir_value = match config.backend {
        crate::plan::schema::BackendType::Container => "/repo/.work".to_string(),
        crate::plan::schema::BackendType::Native => config.work_dir.display().to_string(),
    };
    env.insert("LOOM_WORK_DIR".to_string(), json!(work_dir_value));

    // IMPORTANT: Remove any stale LOOM_MAIN_AGENT_PID from settings.json
    // This variable must be set dynamically by the wrapper script (export LOOM_MAIN_AGENT_PID=$$)
    // so it reflects the actual Claude process PID. A stale value from a previous session
    // in settings.json would cause the commit-filter hook to incorrectly detect the main
    // agent as a subagent.
    env.remove("LOOM_MAIN_AGENT_PID");

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

/// Compute the host path of the per-session container settings overlay.
///
/// Container-backed sessions that run in the main repo (knowledge, merge,
/// base-conflict) must NOT have their settings written into the host's
/// `<repo>/.claude/settings.local.json` — that file is also read by the
/// operator's host Claude sessions, and rewriting it with container-side
/// hook paths (`/home/loom/...`) or `defaultMode: bypassPermissions` breaks
/// parallel host work. We instead write to a loom-owned file under `.work/`
/// and ro-mount it into the container at the expected location.
pub fn container_main_settings_path(work_dir: &Path, session_id: &str) -> PathBuf {
    work_dir
        .join("container-settings")
        .join(format!("{session_id}.local.json"))
}

/// Set up the per-session container-side settings file for a non-worktree
/// container session (knowledge / merge / base-conflict).
///
/// Writes a fresh settings document (no host-file merge) to
/// `<work_dir>/container-settings/<session_id>.local.json`. The container
/// backend mounts this file ro at `/repo/.claude/settings.local.json`,
/// shadowing the host's settings.local.json without modifying it.
///
/// Returns the host path of the written file.
pub fn setup_container_main_session_settings(
    work_dir: &Path,
    config: &HooksConfig,
) -> Result<PathBuf> {
    let settings_path = container_main_settings_path(work_dir, &config.session_id);

    let parent = settings_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("settings path has no parent"))?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "Failed to create container-settings directory: {}",
            parent.display()
        )
    })?;

    // Generate from scratch — we don't want to inherit anything from the
    // host's settings.local.json. The container session is fully described
    // by `config` (backend, permission mode, paths).
    let settings = generate_hooks_settings(config, None)?;

    let content = serde_json::to_string_pretty(&settings)
        .with_context(|| "Failed to serialize container-side settings")?;
    std::fs::write(&settings_path, content).with_context(|| {
        format!(
            "Failed to write container-side settings: {}",
            settings_path.display()
        )
    })?;

    Ok(settings_path)
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
    use crate::plan::schema::{BackendType, PermissionMode};
    use tempfile::TempDir;

    #[test]
    fn test_generate_hooks_settings_container_includes_global_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let config = HooksConfig::new(
            temp_dir.path().to_path_buf(),
            "test-stage".to_string(),
            "test-session".to_string(),
            temp_dir.path().to_path_buf(),
            PermissionMode::Default,
            BackendType::Container,
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
            commands
                .iter()
                .any(|c| c.contains("commit-filter.sh") && c.starts_with("/home/loom/")),
            "commit-filter.sh should use /home/loom/ path, got: {commands:?}"
        );
        assert!(
            commands
                .iter()
                .any(|c| c.contains("prefer-modern-tools.sh") && c.starts_with("/home/loom/")),
            "prefer-modern-tools.sh should use /home/loom/ path, got: {commands:?}"
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
            stop_commands
                .iter()
                .any(|c| c.contains("commit-guard.sh") && c.starts_with("/home/loom/")),
            "commit-guard.sh should be in Stop with /home/loom/ path, got: {stop_commands:?}"
        );
    }

    #[test]
    fn container_main_session_settings_isolates_from_host_repo() {
        // Regression: a container-backed knowledge stage MUST NOT rewrite the
        // host repo's `.claude/settings.local.json`. The per-session overlay
        // belongs under `<work_dir>/container-settings/<session>.local.json`
        // and must contain the container hook paths (`/home/loom/...`) and
        // `bypassPermissions` mode — without ever touching the host file.
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();
        let work_dir = repo_root.join(".work");
        std::fs::create_dir_all(&work_dir).unwrap();

        // Pre-create a host settings.local.json with native (host) hook
        // paths and a non-bypass default mode so we can prove it is left
        // untouched by the container-side write.
        let host_settings = repo_root.join(".claude/settings.local.json");
        std::fs::create_dir_all(host_settings.parent().unwrap()).unwrap();
        let host_marker = serde_json::json!({
            "permissions": {
                "defaultMode": "acceptEdits",
                "allow": ["Read(.work/**)"]
            },
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "/Users/operator/.claude/hooks/loom/commit-filter.sh"
                    }]
                }]
            }
        });
        std::fs::write(
            &host_settings,
            serde_json::to_string_pretty(&host_marker).unwrap(),
        )
        .unwrap();

        let config = HooksConfig::new(
            tmp.path().join("hooks"),
            "kn-bootstrap".to_string(),
            "session-xyz".to_string(),
            work_dir.clone(),
            PermissionMode::BypassPermissions,
            BackendType::Container,
        );

        let written = setup_container_main_session_settings(&work_dir, &config).unwrap();

        // 1. The per-session overlay is in the expected location.
        let expected = work_dir
            .join("container-settings")
            .join("session-xyz.local.json");
        assert_eq!(written, expected);
        assert!(written.exists(), "overlay file must be created");

        // 2. Overlay contains container hook paths + bypassPermissions.
        let overlay_content = std::fs::read_to_string(&written).unwrap();
        let overlay: serde_json::Value = serde_json::from_str(&overlay_content).unwrap();
        assert_eq!(overlay["permissions"]["defaultMode"], "bypassPermissions");
        let pre_tool_use = overlay["hooks"]["PreToolUse"].as_array().unwrap();
        let has_container_path = pre_tool_use.iter().any(|entry| {
            entry["hooks"]
                .as_array()
                .map(|hs| {
                    hs.iter().any(|h| {
                        h["command"]
                            .as_str()
                            .map(|c| c.starts_with("/home/loom/"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        });
        assert!(
            has_container_path,
            "overlay PreToolUse must use container hook paths"
        );

        // 3. Host settings.local.json is UNCHANGED.
        let host_after = std::fs::read_to_string(&host_settings).unwrap();
        let host_after_json: serde_json::Value = serde_json::from_str(&host_after).unwrap();
        assert_eq!(
            host_after_json, host_marker,
            "host repo .claude/settings.local.json must not be modified by container session setup"
        );
    }
}
