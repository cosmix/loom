//! Settings file management for loom permissions
//!
//! # Settings File
//!
//! Loom uses `.claude/settings.json` for project-wide permissions and hooks.
//!
//! ## `settings.json` - Project-wide permissions and hooks
//!
//! Contains permissions and hooks that apply to all Claude Code sessions in the project.
//! This file is checked into the repository and shared across the team.
//!
//! - **Permissions**: File access rules (e.g., `Read(.work/**)`, `Bash(loom:*)`)
//! - **Hooks**: Global event-triggered scripts (e.g., `commit-guard.sh`, `ask-user-pre.sh`)
//!
//! Created/updated by `loom init`. Worktrees merge this with session-specific hooks at creation time.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use super::constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
use super::hooks::{configure_loom_hooks, install_loom_hooks, loom_hooks_config};

/// Ensure `.claude/settings.json` has loom permissions and hooks configured
///
/// This function:
/// 1. Installs loom hook scripts to ~/.claude/hooks/
/// 2. Creates `.claude/` directory if it doesn't exist
/// 3. Creates `settings.json` if it doesn't exist
/// 4. Merges loom permissions into existing file without duplicates
/// 5. Configures global loom hooks (referencing ~/.claude/hooks/*.sh)
///
/// Worktrees will merge this global config with session-specific hooks at creation time.
pub fn ensure_loom_permissions(repo_root: &Path) -> Result<()> {
    // Install loom hooks to ~/.claude/hooks/ and ~/.claude/hooks/loom/
    let hooks_installed = install_loom_hooks()?;
    if hooks_installed > 0 {
        println!("  Installed {hooks_installed} loom hook(s) to ~/.claude/hooks/");
    }

    let claude_dir = repo_root.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    // Create .claude directory if needed
    if !claude_dir.exists() {
        fs::create_dir_all(&claude_dir).with_context(|| {
            format!(
                "Failed to create .claude directory at {}",
                claude_dir.display()
            )
        })?;
    }

    // Load existing settings or create new
    let mut settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read {}", settings_path.display()))?;

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {} as JSON", settings_path.display()))?
    } else {
        json!({})
    };

    // Ensure settings is an object
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json must be a JSON object"))?;

    // Get or create permissions object
    let permissions = settings_obj
        .entry("permissions")
        .or_insert_with(|| json!({}));

    let permissions_obj = permissions
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    // Get or create allow array
    let allow = permissions_obj.entry("allow").or_insert_with(|| json!([]));

    let allow_arr = allow
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions.allow must be a JSON array"))?;

    // Collect existing permissions as strings for deduplication
    let existing: std::collections::HashSet<String> = allow_arr
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // Add missing loom permissions
    let mut added_permissions = 0;
    for permission in LOOM_PERMISSIONS {
        if !existing.contains(*permission) {
            allow_arr.push(json!(permission));
            added_permissions += 1;
        }
    }

    // Configure hooks (only if not already present)
    let hooks_configured = configure_loom_hooks(settings_obj)?;

    // Configure agent teams environment variable
    let env_obj = settings_obj
        .entry("env")
        .or_insert_with(|| json!({}));
    let env_map = env_obj
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("env must be a JSON object"))?;
    let env_configured = if !env_map.contains_key("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS") {
        env_map.insert(
            "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
            json!("1"),
        );
        true
    } else {
        false
    };

    // Write back if we made any changes
    if added_permissions > 0 || hooks_configured || env_configured {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;

        fs::write(&settings_path, content)
            .with_context(|| format!("Failed to write {}", settings_path.display()))?;

        if added_permissions > 0 {
            println!("  Updated .claude/settings.json with {added_permissions} loom permission(s)");
        }
        if hooks_configured {
            println!("  Configured loom hooks in .claude/settings.json");
        }
        if env_configured {
            println!("  Configured agent teams environment variable in .claude/settings.json");
        }
    } else {
        println!("  Claude Code permissions and hooks already configured");
    }

    Ok(())
}

/// Create `.claude/settings.json` for a worktree with worktree-specific permissions
///
/// This creates a NEW settings file (not symlinked) with permissions that use
/// parent traversal (../../.work/**) since worktrees are at .worktrees/stage-X/
/// and .work is symlinked to ../../.work
pub fn create_worktree_settings(worktree_path: &Path) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    // Create .claude directory if needed
    if !claude_dir.exists() {
        fs::create_dir_all(&claude_dir).with_context(|| {
            format!(
                "Failed to create .claude directory at {}",
                claude_dir.display()
            )
        })?;
    }

    // Generate settings with worktree-specific permissions and hooks
    let settings = json!({
        "permissions": {
            "allow": LOOM_PERMISSIONS_WORKTREE
        },
        "hooks": loom_hooks_config(),
        "env": {
            "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
        }
    });

    let content = serde_json::to_string_pretty(&settings)
        .context("Failed to serialize worktree settings to JSON")?;

    fs::write(&settings_path, content)
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_loom_permissions_creates_env_var() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Call ensure_loom_permissions
        ensure_loom_permissions(repo_root).unwrap();

        // Read back settings.json
        let settings_path = repo_root.join(".claude/settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Verify env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS == "1"
        assert_eq!(
            settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"],
            "1"
        );
    }

    #[test]
    fn test_create_worktree_settings_includes_env_var() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        // Call create_worktree_settings
        create_worktree_settings(worktree_path).unwrap();

        // Read back settings.json
        let settings_path = worktree_path.join(".claude/settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Verify env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS == "1"
        assert_eq!(
            settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"],
            "1"
        );
    }

    #[test]
    fn test_ensure_loom_permissions_preserves_existing_env_vars() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create .claude directory
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Pre-create settings.json with custom env var
        let initial_settings = json!({
            "env": {
                "MY_CUSTOM_VAR": "hello"
            }
        });
        let settings_path = claude_dir.join("settings.json");
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&initial_settings).unwrap(),
        )
        .unwrap();

        // Call ensure_loom_permissions
        ensure_loom_permissions(repo_root).unwrap();

        // Read back settings.json
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Verify both env vars exist
        assert_eq!(settings["env"]["MY_CUSTOM_VAR"], "hello");
        assert_eq!(
            settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"],
            "1"
        );
    }
}
