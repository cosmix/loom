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
use super::hooks::{configure_loom_hooks, install_loom_hooks, install_loom_hooks_to};

/// Per-session identity env vars that must NEVER be persisted in settings files.
///
/// These are set dynamically by the session wrapper script (`export LOOM_...`
/// before `exec claude`) so they always reflect the actual running session.
/// Settings-file `env` blocks override the process environment, so a persisted
/// value from an earlier session silently shadows the wrapper's fresh exports:
/// `loom memory` files entries under the wrong stage, hooks heartbeat the wrong
/// session, and commit-filter misidentifies the main agent.
pub const SESSION_IDENTITY_ENV_KEYS: &[&str] =
    &["LOOM_MAIN_AGENT_PID", "LOOM_STAGE_ID", "LOOM_SESSION_ID"];

/// Remove per-session identity env vars from a settings document.
///
/// Returns `true` if any key was removed. Missing or non-object `env` blocks
/// are left untouched.
pub fn scrub_session_identity_env(settings: &mut Value) -> bool {
    let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let mut removed = false;
    for key in SESSION_IDENTITY_ENV_KEYS {
        removed |= env.remove(*key).is_some();
    }
    removed
}

/// Heal the MAIN repo's settings files of stale per-session identity env.
///
/// Claude Code applies the main repository's settings env to sessions running
/// in linked worktrees, so stale identity in either main-repo settings file
/// shadows the wrapper script's fresh exports in EVERY session of this repo —
/// worktree stages included. Scrubbing the worktree-side copies is therefore
/// not enough; the main files must be healed in the run path, not only on
/// `loom init`/`loom repair` (which polluted repos may never re-run).
///
/// Best-effort: missing or unparseable files are skipped. Returns the paths
/// that were healed.
pub fn scrub_main_repo_settings_identity(repo_root: &Path) -> Vec<std::path::PathBuf> {
    let mut healed = Vec::new();
    for name in ["settings.json", "settings.local.json"] {
        let path = repo_root.join(".claude").join(name);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut settings) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        if !scrub_session_identity_env(&mut settings) {
            continue;
        }
        let Ok(updated) = serde_json::to_string_pretty(&settings) else {
            continue;
        };
        if fs::write(&path, updated).is_ok() {
            healed.push(path);
        }
    }
    healed
}

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
    ensure_loom_permissions_to(repo_root, None)
}

/// Testable variant: pass `Some(dir)` to redirect hook installation to a temp directory.
/// Production callers use `ensure_loom_permissions` which passes `None` (installs to ~/.claude/hooks/loom/).
pub fn ensure_loom_permissions_to(repo_root: &Path, hooks_dir: Option<&Path>) -> Result<()> {
    // Install loom hooks
    let hooks_installed = match hooks_dir {
        Some(dir) => install_loom_hooks_to(dir)?,
        None => install_loom_hooks()?,
    };
    if hooks_installed > 0 {
        println!("  Installed {hooks_installed} loom hook(s)");
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

    // Drop stale per-session identity env vars left behind by older loom
    // versions (they used to be written here by knowledge-stage spawns and
    // would shadow the wrapper script's fresh exports in every later session).
    let stale_env_removed = scrub_session_identity_env(&mut settings);
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.local.json must be a JSON object"))?;

    // Disable Claude Code's worktree isolation for subagents in the main repo.
    //
    // Knowledge stages (and interactive sessions) run in the main checkout
    // rather than a loom worktree. With Claude Code's default bgIsolation
    // ("worktree"), their subagents would be forced into nested git worktrees,
    // leaving stray branches behind. "none" lets subagents edit the checkout
    // directly. Worktree stage sessions get the same setting via the sandbox
    // settings generator. (Claude Code v2.1.143+; older versions ignore it.)
    let worktree_obj = settings_obj.entry("worktree").or_insert_with(|| json!({}));
    let worktree_map = worktree_obj
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("worktree must be a JSON object"))?;
    let worktree_configured = if worktree_map.get("bgIsolation") != Some(&json!("none")) {
        worktree_map.insert("bgIsolation".to_string(), json!("none"));
        true
    } else {
        false
    };

    // Write back if we made any changes
    if hooks_configured || env_configured || worktree_configured || stale_env_removed {
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
        if worktree_configured {
            println!("  Disabled Claude Code worktree isolation in .claude/settings.local.json");
        }
        if stale_env_removed {
            println!("  Removed stale session env vars from .claude/settings.local.json");
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

    // Remove CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS and stale per-session
    // identity from env in settings.json (very old loom versions persisted
    // identity here; it shadows the wrapper's exports in every session)
    if let Some(env) = settings_obj.get_mut("env").and_then(|v| v.as_object_mut()) {
        if env.remove("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS").is_some() {
            migrated = true;
        }
        for key in SESSION_IDENTITY_ENV_KEYS {
            if env.remove(*key).is_some() {
                migrated = true;
            }
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
        let hooks_dir = temp_dir.path().join("hooks");

        ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

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
        let hooks_dir = temp_dir.path().join("hooks");

        ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

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
    fn test_ensure_loom_disables_worktree_isolation_in_settings_local() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let hooks_dir = temp_dir.path().join("hooks");

        ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

        let settings_local_path = repo_root.join(".claude/settings.local.json");
        let content = fs::read_to_string(&settings_local_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Worktree isolation must be off so main-repo subagents (knowledge
        // stages, interactive sessions) don't spawn nested worktrees.
        assert_eq!(settings["worktree"]["bgIsolation"], "none");

        // Running again is idempotent — the value is already "none".
        ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();
        let content = fs::read_to_string(&settings_local_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(settings["worktree"]["bgIsolation"], "none");
    }

    #[test]
    fn test_ensure_loom_permissions_preserves_existing_settings_local() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let hooks_dir = temp_dir.path().join("hooks");

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

        ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

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

        ensure_loom_permissions_to(repo_root, Some(&temp_dir.path().join("hooks"))).unwrap();

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
    fn test_migrate_removes_session_identity_from_settings_json() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Very old loom versions persisted session identity in settings.json
        let old_settings = json!({
            "permissions": { "allow": ["Bash(loom *)"] },
            "env": {
                "LOOM_STAGE_ID": "knowledge-bootstrap",
                "LOOM_SESSION_ID": "session-stale",
                "LOOM_WORK_DIR": "/repo/.work"
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&old_settings).unwrap(),
        )
        .unwrap();

        ensure_loom_permissions_to(repo_root, Some(&temp_dir.path().join("hooks"))).unwrap();

        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        let env = settings["env"].as_object().unwrap();
        assert!(!env.contains_key("LOOM_STAGE_ID"));
        assert!(!env.contains_key("LOOM_SESSION_ID"));
        // Stable, repo-scoped value survives
        assert_eq!(env["LOOM_WORK_DIR"], "/repo/.work");
    }

    #[test]
    fn test_scrub_main_repo_settings_identity_heals_both_files() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let polluted = json!({
            "env": {
                "LOOM_STAGE_ID": "knowledge-bootstrap",
                "LOOM_SESSION_ID": "session-stale",
                "LOOM_MAIN_AGENT_PID": "12345",
                "LOOM_WORK_DIR": "/repo/.work"
            },
            "permissions": { "allow": ["Bash(loom *)"] }
        });
        for name in ["settings.json", "settings.local.json"] {
            fs::write(
                claude_dir.join(name),
                serde_json::to_string_pretty(&polluted).unwrap(),
            )
            .unwrap();
        }

        let healed = scrub_main_repo_settings_identity(repo_root);
        assert_eq!(healed.len(), 2);

        for name in ["settings.json", "settings.local.json"] {
            let content = fs::read_to_string(claude_dir.join(name)).unwrap();
            let settings: Value = serde_json::from_str(&content).unwrap();
            let env = settings["env"].as_object().unwrap();
            assert!(!env.contains_key("LOOM_STAGE_ID"), "{name}");
            assert!(!env.contains_key("LOOM_SESSION_ID"), "{name}");
            assert!(!env.contains_key("LOOM_MAIN_AGENT_PID"), "{name}");
            assert_eq!(env["LOOM_WORK_DIR"], "/repo/.work", "{name}");
            // Unrelated sections untouched
            let allow = settings["permissions"]["allow"].as_array().unwrap();
            assert!(allow.iter().any(|v| v == "Bash(loom *)"), "{name}");
        }
    }

    #[test]
    fn test_scrub_main_repo_settings_identity_noop_cases() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // No .claude directory at all
        assert!(scrub_main_repo_settings_identity(repo_root).is_empty());

        // Clean file → nothing healed, file byte-identical
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let clean =
            serde_json::to_string_pretty(&json!({ "env": { "LOOM_WORK_DIR": "/repo/.work" } }))
                .unwrap();
        fs::write(claude_dir.join("settings.local.json"), &clean).unwrap();
        assert!(scrub_main_repo_settings_identity(repo_root).is_empty());
        assert_eq!(
            fs::read_to_string(claude_dir.join("settings.local.json")).unwrap(),
            clean
        );

        // Unparseable file is skipped without error
        fs::write(claude_dir.join("settings.json"), "{not json").unwrap();
        assert!(scrub_main_repo_settings_identity(repo_root).is_empty());
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
