//! Settings file management for loom permissions
//!
//! # Settings Files
//!
//! Loom uses two settings files in `.claude/`:
//!
//! ## `settings.json` - Team-shared permissions (committed to git)
//!
//! Contains generic permissions that apply to all Claude Code sessions in the project.
//! This file is safe to commit because it contains no user-specific paths.
//!
//! - **Permissions**: File access rules (e.g., `Read(.work/**)`, `Bash(loom *)`)
//!
//! ## `settings.local.json` - User-local hooks and env (gitignored)
//!
//! Contains hooks and environment variables that reference user-specific paths
//! (e.g., `~/.claude/hooks/loom/`). This file is NOT committed to the repository.
//!
//! - **Hooks**: Global event-triggered scripts (e.g., `commit-guard.sh`, `ask-user-pre.sh`)
//! - **Env**: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` and other loom env vars
//!
//! Created/updated by `loom init`. Worktrees merge this with session-specific hooks at creation time.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use super::constants::LOOM_PERMISSIONS;
use super::hooks::{configure_loom_hooks, install_loom_hooks};

/// Ensure `.claude/settings.json` has loom permissions configured
///
/// This function:
/// 1. Installs loom hook scripts to ~/.claude/hooks/loom/
/// 2. Creates `.claude/` directory if it doesn't exist
/// 3. Creates `settings.json` if it doesn't exist
/// 4. Merges loom permissions into existing file without duplicates
/// 5. Migrates hooks/env from settings.json to settings.local.json (if present)
/// 6. Writes hooks + env to settings.local.json
///
/// Worktrees will merge this config with session-specific hooks at creation time.
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

    // Migrate: remove hooks and env from settings.json (they belong in settings.local.json)
    let migrated = migrate_hooks_to_local(settings_obj);

    // Write back if we made any changes
    if added_permissions > 0 || migrated {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;

        fs::write(&settings_path, content)
            .with_context(|| format!("Failed to write {}", settings_path.display()))?;

        if added_permissions > 0 {
            println!("  Updated .claude/settings.json with {added_permissions} loom permission(s)");
        }
        if migrated {
            println!(
                "  Migrated hooks/env from .claude/settings.json to .claude/settings.local.json"
            );
        }
    } else {
        println!("  Claude Code permissions already configured");
    }

    // Write hooks and env to settings.local.json
    ensure_loom_hooks_local(repo_root)?;

    Ok(())
}

/// Write loom hooks and env vars to `.claude/settings.local.json`
///
/// This merges loom hooks and environment variables into the existing
/// settings.local.json (which may already contain sandbox config and
/// runtime permissions). User-specific paths in hooks make this file
/// unsuitable for committing to git.
pub fn ensure_loom_hooks_local(repo_root: &Path) -> Result<()> {
    let claude_dir = repo_root.join(".claude");
    let settings_local_path = claude_dir.join("settings.local.json");

    // Create .claude directory if needed
    if !claude_dir.exists() {
        fs::create_dir_all(&claude_dir).with_context(|| {
            format!(
                "Failed to create .claude directory at {}",
                claude_dir.display()
            )
        })?;
    }

    // Load existing settings.local.json or create new
    let mut settings: Value = if settings_local_path.exists() {
        let content = fs::read_to_string(&settings_local_path)
            .with_context(|| format!("Failed to read {}", settings_local_path.display()))?;

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {} as JSON", settings_local_path.display()))?
    } else {
        json!({})
    };

    // Ensure settings is an object
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.local.json must be a JSON object"))?;

    // Configure hooks
    let hooks_configured = configure_loom_hooks(settings_obj)?;

    // Configure agent teams environment variable
    let env_obj = settings_obj.entry("env").or_insert_with(|| json!({}));
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
    if hooks_configured || env_configured {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings.local.json to JSON")?;

        fs::write(&settings_local_path, content)
            .with_context(|| format!("Failed to write {}", settings_local_path.display()))?;

        if hooks_configured {
            println!("  Configured loom hooks in .claude/settings.local.json");
        }
        if env_configured {
            println!("  Configured agent teams env var in .claude/settings.local.json");
        }
    } else {
        println!("  Hooks and env vars already configured in .claude/settings.local.json");
    }

    Ok(())
}

/// Migrate hooks and env from settings.json to settings.local.json
///
/// If settings.json contains a `hooks` key or `env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`,
/// remove them (they will be recreated in settings.local.json by `ensure_loom_hooks_local`).
///
/// Returns true if any migration was performed.
fn migrate_hooks_to_local(settings_obj: &mut serde_json::Map<String, Value>) -> bool {
    let mut migrated = false;

    // Remove hooks from settings.json
    if settings_obj.remove("hooks").is_some() {
        migrated = true;
    }

    // Remove CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS from env in settings.json
    if let Some(env) = settings_obj.get_mut("env").and_then(|v| v.as_object_mut()) {
        if env.remove("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS").is_some() {
            migrated = true;
        }
        // If env is now empty, remove it entirely
        if env.is_empty() {
            settings_obj.remove("env");
        }
    }

    migrated
}

/// Check if settings.json still contains hooks that should be in settings.local.json
pub fn settings_json_has_hooks(repo_root: &Path) -> bool {
    let settings_path = repo_root.join(".claude/settings.json");
    if !settings_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let settings: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    settings.get("hooks").is_some()
}

/// Check if settings.local.json has hooks and env configured
pub fn settings_local_has_hooks(repo_root: &Path) -> bool {
    let settings_local_path = repo_root.join(".claude/settings.local.json");
    if !settings_local_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(&settings_local_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let settings: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let has_hooks = settings.get("hooks").is_some();
    let has_env = settings
        .get("env")
        .and_then(|e| e.get("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"))
        .is_some();

    has_hooks && has_env
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_loom_permissions_creates_settings_json_with_permissions_only() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        ensure_loom_permissions(repo_root).unwrap();

        // settings.json should have permissions but NOT hooks or env
        let settings_path = repo_root.join(".claude/settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Permissions should be present
        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert!(allow.iter().any(|v| v == "Bash(loom *)"));

        // Hooks should NOT be in settings.json
        assert!(settings.get("hooks").is_none());

        // Env should NOT be in settings.json
        assert!(settings.get("env").is_none());
    }

    #[test]
    fn test_ensure_loom_permissions_creates_hooks_in_settings_local() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        ensure_loom_permissions(repo_root).unwrap();

        // settings.local.json should have hooks and env
        let settings_local_path = repo_root.join(".claude/settings.local.json");
        assert!(settings_local_path.exists());

        let content = fs::read_to_string(&settings_local_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Hooks should be present
        assert!(settings.get("hooks").is_some());

        // Env should be present
        assert_eq!(settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"], "1");
    }

    #[test]
    fn test_ensure_loom_permissions_preserves_existing_settings_local() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create .claude directory and pre-existing settings.local.json with sandbox config
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let existing = json!({
            "permissions": {
                "allow": ["Read(src/**)"]
            },
            "sandbox": {
                "enabled": true
            }
        });
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        ensure_loom_permissions(repo_root).unwrap();

        // Read back settings.local.json
        let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Existing sandbox config should be preserved
        assert_eq!(settings["sandbox"]["enabled"], true);

        // Existing permissions should be preserved
        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert!(allow.iter().any(|v| v == "Read(src/**)"));

        // Hooks should be added
        assert!(settings.get("hooks").is_some());

        // Env should be added
        assert_eq!(settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"], "1");
    }

    #[test]
    fn test_migrate_hooks_from_settings_json() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create .claude directory with settings.json that has old hooks + env
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let old_settings = json!({
            "permissions": {
                "allow": ["Bash(loom *)"]
            },
            "hooks": {
                "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "/home/user/.claude/hooks/loom/commit-filter.sh"}]}]
            },
            "env": {
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&old_settings).unwrap(),
        )
        .unwrap();

        ensure_loom_permissions(repo_root).unwrap();

        // settings.json should no longer have hooks or env
        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        assert!(settings.get("hooks").is_none());
        assert!(settings.get("env").is_none());

        // settings.local.json should have hooks and env
        let local_content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
        let local_settings: Value = serde_json::from_str(&local_content).unwrap();
        assert!(local_settings.get("hooks").is_some());
        assert_eq!(
            local_settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"],
            "1"
        );
    }

    #[test]
    fn test_settings_json_has_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // No hooks
        let settings = json!({"permissions": {"allow": []}});
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();
        assert!(!settings_json_has_hooks(repo_root));

        // With hooks
        let settings = json!({"hooks": {"PreToolUse": []}});
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();
        assert!(settings_json_has_hooks(repo_root));
    }
}
