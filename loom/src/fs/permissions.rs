//! Claude Code permissions management for loom
//!
//! Ensures that `.claude/settings.local.json` has the necessary permissions
//! and hooks for loom to operate without constant user approval prompts.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Embedded loom stop hook script
/// This script enforces commit and stage completion in loom worktrees
const LOOM_STOP_HOOK: &str = include_str!("../../resources/hooks/loom-stop.sh");

/// Loom permissions for the MAIN REPO context
/// Includes worktree permissions so settings.local.json can be symlinked to worktrees
/// and all sessions share the same permission file (approvals propagate)
pub const LOOM_PERMISSIONS: &[&str] = &[
    // Read/write access via symlink path (for worktree sessions via symlink)
    "Read(.work/**)",
    "Write(.work/**)",
    // Read/write access via parent traversal (for worktree sessions via direct path)
    "Read(../../.work/**)",
    "Write(../../.work/**)",
    // Read access to CLAUDE.md files (subagents need to read these explicitly)
    "Read(.claude/**)",
    "Read(~/.claude/**)",
    // Loom CLI commands (use :* for prefix matching)
    "Bash(loom:*)",
    // Tmux for session management
    "Bash(tmux:*)",
];

/// Generate hooks configuration for loom
/// Hooks reference scripts at ~/.claude/hooks/ (installed by loom init)
fn loom_hooks_config() -> Value {
    json!({
        "PreToolUse": [
            {
                "matcher": "AskUserQuestion",
                "hooks": [
                    {
                        "type": "command",
                        "command": "~/.claude/hooks/ask-user-pre.sh"
                    }
                ]
            }
        ],
        "PostToolUse": [
            {
                "matcher": "AskUserQuestion",
                "hooks": [
                    {
                        "type": "command",
                        "command": "~/.claude/hooks/ask-user-post.sh"
                    }
                ]
            }
        ],
        "Stop": [
            {
                "hooks": [
                    {
                        "type": "command",
                        "command": "~/.claude/hooks/loom-stop.sh"
                    }
                ]
            }
        ]
    })
}

/// Install the loom stop hook to ~/.claude/hooks/
///
/// This creates the hook script that enforces commit and stage completion
/// in loom worktrees before allowing Claude to stop.
///
/// # Returns
/// - `Ok(true)` if the hook was installed or updated
/// - `Ok(false)` if the hook already exists and is up to date
/// - `Err` if installation failed
pub fn install_loom_hooks() -> Result<bool> {
    let home_dir = dirs::home_dir().context("Failed to determine home directory")?;
    let hooks_dir = home_dir.join(".claude/hooks");
    let hook_path = hooks_dir.join("loom-stop.sh");

    // Create hooks directory if needed
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir).with_context(|| {
            format!(
                "Failed to create hooks directory at {}",
                hooks_dir.display()
            )
        })?;
    }

    // Check if hook already exists with same content
    if hook_path.exists() {
        let existing_content = fs::read_to_string(&hook_path)
            .with_context(|| format!("Failed to read existing hook at {}", hook_path.display()))?;

        if existing_content == LOOM_STOP_HOOK {
            return Ok(false); // Already up to date
        }
    }

    // Write the hook script
    fs::write(&hook_path, LOOM_STOP_HOOK)
        .with_context(|| format!("Failed to write hook to {}", hook_path.display()))?;

    // Make executable (chmod +x)
    let mut perms = fs::metadata(&hook_path)
        .with_context(|| format!("Failed to get metadata for {}", hook_path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms)
        .with_context(|| format!("Failed to set permissions on {}", hook_path.display()))?;

    Ok(true)
}

/// Loom permissions for WORKTREE context
/// Includes both .work/** (symlink path as seen by Claude) and ../../.work/** (parent traversal)
/// The symlink at .worktrees/stage-X/.work -> ../../.work means Claude sees paths as .work/**
/// but the actual files are accessed via parent traversal
pub const LOOM_PERMISSIONS_WORKTREE: &[&str] = &[
    // Read/write access via symlink path (how Claude sees and requests the paths)
    "Read(.work/**)",
    "Write(.work/**)",
    // Read/write access via parent traversal (alternative direct access pattern)
    "Read(../../.work/**)",
    "Write(../../.work/**)",
    // Read access to CLAUDE.md files (subagents need to read these explicitly)
    "Read(.claude/**)",
    "Read(~/.claude/**)",
    // Loom CLI commands (use :* for prefix matching)
    "Bash(loom:*)",
    // Tmux for session management
    "Bash(tmux:*)",
];

/// Ensure `.claude/settings.local.json` has loom permissions and hooks configured
///
/// This function:
/// 1. Installs loom hook scripts to ~/.claude/hooks/
/// 2. Creates `.claude/` directory if it doesn't exist
/// 3. Creates `settings.local.json` if it doesn't exist
/// 4. Merges loom permissions into existing file without duplicates
/// 5. Configures loom hooks (referencing ~/.claude/hooks/*.sh)
///
/// Since worktrees symlink `.claude/` to the main repo, these permissions
/// automatically propagate to all loom sessions.
pub fn ensure_loom_permissions(repo_root: &Path) -> Result<()> {
    // First, install loom hooks to ~/.claude/hooks/
    let hooks_installed = install_loom_hooks()?;
    if hooks_installed {
        println!("  Installed loom-stop.sh hook to ~/.claude/hooks/");
    }

    let claude_dir = repo_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

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
        .ok_or_else(|| anyhow::anyhow!("settings.local.json must be a JSON object"))?;

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

    // Write back if we made any changes
    if added_permissions > 0 || hooks_configured {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;

        fs::write(&settings_path, content)
            .with_context(|| format!("Failed to write {}", settings_path.display()))?;

        if added_permissions > 0 {
            println!(
                "  Updated .claude/settings.local.json with {added_permissions} loom permission(s)"
            );
        }
        if hooks_configured {
            println!("  Configured loom hooks in .claude/settings.local.json");
        }
    } else {
        println!("  Claude Code permissions and hooks already configured");
    }

    Ok(())
}

/// Configure loom hooks in settings object
/// Returns true if hooks were added/updated, false if already configured
fn configure_loom_hooks(settings_obj: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let loom_hooks = loom_hooks_config();

    // Check if hooks already exist
    if let Some(existing_hooks) = settings_obj.get("hooks") {
        // Check if loom hooks are already configured by looking for our specific hooks
        if let Some(hooks_obj) = existing_hooks.as_object() {
            // Check for Stop hook with loom-stop.sh as marker
            if let Some(stop_hooks) = hooks_obj.get("Stop") {
                if let Some(stop_arr) = stop_hooks.as_array() {
                    for hook_entry in stop_arr {
                        if let Some(hooks) = hook_entry.get("hooks").and_then(|h| h.as_array()) {
                            for hook in hooks {
                                if let Some(cmd) = hook.get("command").and_then(|c| c.as_str()) {
                                    if cmd.contains("loom-stop.sh") {
                                        // Loom hooks already configured
                                        return Ok(false);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Merge loom hooks into existing hooks or create new
    let hooks = settings_obj.entry("hooks").or_insert_with(|| json!({}));

    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks must be a JSON object"))?;

    // Add each hook type from loom config
    if let Some(loom_hooks_obj) = loom_hooks.as_object() {
        for (event_name, event_hooks) in loom_hooks_obj {
            let event_arr = hooks_obj
                .entry(event_name)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("hooks.{event_name} must be an array"))?;

            // Add loom hooks to the array
            if let Some(new_hooks) = event_hooks.as_array() {
                for hook in new_hooks {
                    event_arr.push(hook.clone());
                }
            }
        }
    }

    Ok(true)
}

/// Create `.claude/settings.local.json` for a worktree with worktree-specific permissions
///
/// This creates a NEW settings file (not symlinked) with permissions that use
/// parent traversal (../../.work/**) since worktrees are at .worktrees/stage-X/
/// and .work is symlinked to ../../.work
pub fn create_worktree_settings(worktree_path: &Path) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

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
        "hooks": loom_hooks_config()
    });

    let content = serde_json::to_string_pretty(&settings)
        .context("Failed to serialize worktree settings to JSON")?;

    fs::write(&settings_path, content)
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;

    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_loom_permissions_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        ensure_loom_permissions(repo_root).unwrap();

        let settings_path = repo_root.join(".claude/settings.local.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert!(allow.iter().any(|v| v == "Bash(loom:*)"));
        assert!(allow.iter().any(|v| v == "Bash(tmux:*)"));
    }

    #[test]
    fn test_ensure_loom_permissions_merges_existing() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Create existing settings with some permissions
        let existing = json!({
            "permissions": {
                "allow": ["Read(src/**)"],
                "deny": ["Bash(rm -rf:*)"]
            },
            "other_setting": true
        });
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        ensure_loom_permissions(repo_root).unwrap();

        let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Check existing permissions preserved
        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert!(allow.iter().any(|v| v == "Read(src/**)"));

        // Check loom CLI permissions added
        assert!(allow.iter().any(|v| v == "Bash(loom:*)"));
        assert!(allow.iter().any(|v| v == "Bash(tmux:*)"));

        // Check deny list preserved
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert!(deny.iter().any(|v| v == "Bash(rm -rf:*)"));

        // Check other settings preserved
        assert_eq!(settings["other_setting"], true);
    }

    #[test]
    fn test_ensure_loom_permissions_no_duplicates() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let claude_dir = repo_root.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Create existing settings with some loom permissions already
        let existing = json!({
            "permissions": {
                "allow": ["Bash(loom:*)", "Bash(tmux:*)"]
            }
        });
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        ensure_loom_permissions(repo_root).unwrap();

        let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let allow = settings["permissions"]["allow"].as_array().unwrap();

        // Count occurrences of Bash(loom:*) - should be exactly 1
        let loom_count = allow.iter().filter(|v| *v == "Bash(loom:*)").count();
        assert_eq!(loom_count, 1);
    }

    #[test]
    fn test_loom_permissions_constant() {
        // Main repo includes all permissions (shared with worktrees via symlink)
        assert!(LOOM_PERMISSIONS.contains(&"Bash(loom:*)"));
        assert!(LOOM_PERMISSIONS.contains(&"Bash(tmux:*)"));
        // Now includes worktree permissions so settings can be symlinked
        assert!(LOOM_PERMISSIONS.contains(&"Read(.work/**)"));
        assert!(LOOM_PERMISSIONS.contains(&"Write(.work/**)"));
        assert!(LOOM_PERMISSIONS.contains(&"Read(../../.work/**)"));
        assert!(LOOM_PERMISSIONS.contains(&"Write(../../.work/**)"));
    }

    #[test]
    fn test_worktree_permissions_constant() {
        // Worktree permissions should match main repo permissions
        // (since settings.local.json is now symlinked from main)
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.work/**)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(.work/**)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(../../.work/**)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(../../.work/**)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.claude/**)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/**)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom:*)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(tmux:*)"));
    }

    #[test]
    fn test_hooks_config_structure() {
        let hooks = loom_hooks_config();
        let hooks_obj = hooks.as_object().unwrap();

        // Check PreToolUse hook
        let pre_tool = hooks_obj.get("PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);
        assert_eq!(pre_tool[0]["matcher"], "AskUserQuestion");
        assert!(pre_tool[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("ask-user-pre.sh"));

        // Check PostToolUse hook
        let post_tool = hooks_obj.get("PostToolUse").unwrap().as_array().unwrap();
        assert_eq!(post_tool.len(), 1);
        assert_eq!(post_tool[0]["matcher"], "AskUserQuestion");
        assert!(post_tool[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("ask-user-post.sh"));

        // Check Stop hook
        let stop = hooks_obj.get("Stop").unwrap().as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert!(stop[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("loom-stop.sh"));
    }

    #[test]
    fn test_ensure_loom_permissions_adds_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        ensure_loom_permissions(repo_root).unwrap();

        let settings_path = repo_root.join(".claude/settings.local.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Check hooks are configured
        let hooks = settings.get("hooks").expect("hooks should be present");
        let hooks_obj = hooks.as_object().unwrap();

        assert!(hooks_obj.contains_key("PreToolUse"));
        assert!(hooks_obj.contains_key("PostToolUse"));
        assert!(hooks_obj.contains_key("Stop"));
    }

    #[test]
    fn test_hooks_not_duplicated_on_rerun() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Run twice
        ensure_loom_permissions(repo_root).unwrap();
        ensure_loom_permissions(repo_root).unwrap();

        let settings_path = repo_root.join(".claude/settings.local.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Should still have exactly one Stop hook entry
        let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_hooks.len(), 1);
    }

    #[test]
    fn test_worktree_settings_includes_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        create_worktree_settings(worktree_path).unwrap();

        let settings_path = worktree_path.join(".claude/settings.local.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Check hooks are present
        let hooks = settings.get("hooks").expect("hooks should be present");
        let hooks_obj = hooks.as_object().unwrap();
        assert!(hooks_obj.contains_key("Stop"));
    }

    #[test]
    fn test_embedded_loom_stop_hook_is_valid() {
        // Verify the embedded hook script is non-empty and has correct shebang
        assert!(!LOOM_STOP_HOOK.is_empty());
        assert!(LOOM_STOP_HOOK.starts_with("#!/usr/bin/env bash"));
        // Check for key functions that should exist
        assert!(LOOM_STOP_HOOK.contains("detect_loom_worktree"));
        assert!(LOOM_STOP_HOOK.contains("check_git_clean"));
        assert!(LOOM_STOP_HOOK.contains("get_stage_status"));
        assert!(LOOM_STOP_HOOK.contains("block_with_reason"));
    }

    #[test]
    fn test_install_loom_hooks_creates_hook_file() {
        // This test modifies ~/.claude/hooks/ which is a real directory
        // We'll verify the hook content matches what we expect
        let result = install_loom_hooks();
        assert!(result.is_ok());

        let home_dir = dirs::home_dir().expect("should have home dir");
        let hook_path = home_dir.join(".claude/hooks/loom-stop.sh");

        if hook_path.exists() {
            let content = fs::read_to_string(&hook_path).unwrap();
            // Verify the hook has the expected content
            assert!(content.contains("detect_loom_worktree"));
            assert!(content.contains("LOOM WORKTREE EXIT BLOCKED"));
        }
    }
}
