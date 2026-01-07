//! Claude Code permissions management for loom
//!
//! Ensures that `.claude/settings.local.json` has the necessary permissions
//! for loom to operate without constant user approval prompts.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Loom permissions for the MAIN REPO context
/// .work/ is within repo root so no explicit Read/Write permissions needed
pub const LOOM_PERMISSIONS: &[&str] = &[
    // Loom CLI commands
    "Bash(loom *)",
    "Bash(loom)",
    // Tmux for session management
    "Bash(tmux *)",
];

/// Loom permissions for WORKTREE context
/// Uses parent traversal because .work is symlinked to ../../.work
pub const LOOM_PERMISSIONS_WORKTREE: &[&str] = &[
    // Read/write access to work directory via parent (worktree is at .worktrees/stage-X/)
    "Read(../../.work/**)",
    "Write(../../.work/**)",
    // Loom CLI commands
    "Bash(loom *)",
    "Bash(loom)",
    // Tmux for session management
    "Bash(tmux *)",
];

/// Ensure `.claude/settings.local.json` has loom permissions configured
///
/// This function:
/// 1. Creates `.claude/` directory if it doesn't exist
/// 2. Creates `settings.local.json` if it doesn't exist
/// 3. Merges loom permissions into existing file without duplicates
///
/// Since worktrees symlink `.claude/` to the main repo, these permissions
/// automatically propagate to all loom sessions.
pub fn ensure_loom_permissions(repo_root: &Path) -> Result<()> {
    let claude_dir = repo_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

    // Create .claude directory if needed
    if !claude_dir.exists() {
        fs::create_dir_all(&claude_dir)
            .with_context(|| format!("Failed to create .claude directory at {}", claude_dir.display()))?;
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
    let settings_obj = settings.as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.local.json must be a JSON object"))?;

    // Get or create permissions object
    let permissions = settings_obj
        .entry("permissions")
        .or_insert_with(|| json!({}));

    let permissions_obj = permissions.as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    // Get or create allow array
    let allow = permissions_obj
        .entry("allow")
        .or_insert_with(|| json!([]));

    let allow_arr = allow.as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions.allow must be a JSON array"))?;

    // Collect existing permissions as strings for deduplication
    let existing: std::collections::HashSet<String> = allow_arr
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // Add missing loom permissions
    let mut added_count = 0;
    for permission in LOOM_PERMISSIONS {
        if !existing.contains(*permission) {
            allow_arr.push(json!(permission));
            added_count += 1;
        }
    }

    // Write back if we added any permissions
    if added_count > 0 {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;

        fs::write(&settings_path, content)
            .with_context(|| format!("Failed to write {}", settings_path.display()))?;

        println!("  Updated .claude/settings.local.json with {} loom permission(s)", added_count);
    } else {
        println!("  Claude Code permissions already configured");
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
        fs::create_dir_all(&claude_dir)
            .with_context(|| format!("Failed to create .claude directory at {}", claude_dir.display()))?;
    }

    // Generate settings with worktree-specific permissions
    let settings = json!({
        "permissions": {
            "allow": LOOM_PERMISSIONS_WORKTREE
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
        assert!(allow.iter().any(|v| v == "Bash(loom *)"));
        assert!(allow.iter().any(|v| v == "Bash(loom)"));
        assert!(allow.iter().any(|v| v == "Bash(tmux *)"));
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
                "deny": ["Bash(rm -rf *)"]
            },
            "other_setting": true
        });
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        ).unwrap();

        ensure_loom_permissions(repo_root).unwrap();

        let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Check existing permissions preserved
        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert!(allow.iter().any(|v| v == "Read(src/**)"));

        // Check loom CLI permissions added
        assert!(allow.iter().any(|v| v == "Bash(loom *)"));
        assert!(allow.iter().any(|v| v == "Bash(tmux *)"));

        // Check deny list preserved
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert!(deny.iter().any(|v| v == "Bash(rm -rf *)"));

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
                "allow": ["Bash(loom *)", "Bash(tmux *)"]
            }
        });
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        ).unwrap();

        ensure_loom_permissions(repo_root).unwrap();

        let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let allow = settings["permissions"]["allow"].as_array().unwrap();

        // Count occurrences of Bash(loom *) - should be exactly 1
        let loom_count = allow.iter().filter(|v| *v == "Bash(loom *)").count();
        assert_eq!(loom_count, 1);
    }

    #[test]
    fn test_loom_permissions_constant() {
        // Main repo only needs CLI permissions (.work/ is within repo root)
        assert!(LOOM_PERMISSIONS.contains(&"Bash(loom *)"));
        assert!(LOOM_PERMISSIONS.contains(&"Bash(loom)"));
        assert!(LOOM_PERMISSIONS.contains(&"Bash(tmux *)"));
        // Main repo should NOT have parent traversal permissions
        assert!(!LOOM_PERMISSIONS.iter().any(|p| p.contains("../../")));
    }

    #[test]
    fn test_worktree_permissions_constant() {
        // Worktree needs parent traversal for .work access
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(../../.work/**)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(../../.work/**)"));
        // Worktree also needs CLI permissions
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom *)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom)"));
        assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(tmux *)"));
    }
}
