//! Settings file management for loom permissions

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use super::constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
use super::hooks::{configure_loom_hooks, install_loom_hooks, loom_hooks_config};

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
