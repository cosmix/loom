//! Worktree settings management
//!
//! Handles creation of settings files (.claude/, CLAUDE.md) for worktrees.
//! Also supports hooks configuration when session context is available.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use crate::orchestrator::hooks::{HooksConfig, setup_hooks_for_worktree};

/// Creates or restores the .work symlink in a worktree.
///
/// Used during worktree creation and merge failure recovery.
/// The symlink points from .worktrees/{stage_id}/.work to ../../.work (the main repo's .work/).
pub fn ensure_work_symlink(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    let main_work_dir = repo_root.join(".work");
    let worktree_work_link = worktree_path.join(".work");
    let relative_work_path = Path::new("../../.work");

    if main_work_dir.exists() && !worktree_work_link.exists() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(relative_work_path, &worktree_work_link)
            .with_context(|| "Failed to create .work symlink in worktree")?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(relative_work_path, &worktree_work_link)
            .with_context(|| "Failed to create .work symlink in worktree")?;
    }
    Ok(())
}

/// Set up .claude/ directory for worktree
///
/// We create a real directory and symlink both CLAUDE.md and settings.local.json
/// from main repo. This ensures:
/// 1. Instructions (CLAUDE.md) are shared
/// 2. Permissions (settings.local.json) are shared - approvals propagate across sessions
pub fn setup_claude_directory(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    let main_claude_dir = repo_root.join(".claude");
    let worktree_claude_dir = worktree_path.join(".claude");

    if main_claude_dir.exists() && !worktree_claude_dir.exists() {
        // Create real .claude/ directory in worktree
        std::fs::create_dir_all(&worktree_claude_dir)
            .with_context(|| "Failed to create .claude directory in worktree")?;

        // Symlink CLAUDE.md from main repo for instruction inheritance
        let main_claude_md = main_claude_dir.join("CLAUDE.md");
        if main_claude_md.exists() {
            let worktree_claude_md = worktree_claude_dir.join("CLAUDE.md");
            let relative_claude_md = Path::new("../../../.claude/CLAUDE.md");

            #[cfg(unix)]
            std::os::unix::fs::symlink(relative_claude_md, &worktree_claude_md)
                .with_context(|| "Failed to create CLAUDE.md symlink in worktree")?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(relative_claude_md, &worktree_claude_md)
                .with_context(|| "Failed to create CLAUDE.md symlink in worktree")?;
        }

        // Create settings.local.json with trust and auto-accept settings merged with main repo settings
        let main_settings = main_claude_dir.join("settings.local.json");
        let worktree_settings = worktree_claude_dir.join("settings.local.json");
        create_worktree_settings(&main_settings, &worktree_settings)?;
    }

    Ok(())
}

/// Symlink project-root CLAUDE.md (distinct from .claude/CLAUDE.md)
///
/// This ensures instances in worktrees have access to project instructions
/// without needing to read from the main repo outside the worktree
pub fn setup_root_claude_md(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    let main_root_claude_md = repo_root.join("CLAUDE.md");
    let worktree_root_claude_md = worktree_path.join("CLAUDE.md");

    if main_root_claude_md.exists() && !worktree_root_claude_md.exists() {
        // Relative path from .worktrees/{stage_id}/CLAUDE.md to ../../CLAUDE.md
        let relative_root_claude_md = Path::new("../../CLAUDE.md");

        #[cfg(unix)]
        std::os::unix::fs::symlink(relative_root_claude_md, &worktree_root_claude_md)
            .with_context(|| "Failed to create root CLAUDE.md symlink in worktree")?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(relative_root_claude_md, &worktree_root_claude_md)
            .with_context(|| "Failed to create root CLAUDE.md symlink in worktree")?;
    }

    Ok(())
}

/// Create settings.local.json for a worktree with trust and auto-accept settings.
///
/// This function:
/// 1. Reads the main repo's settings.local.json (if it exists)
/// 2. Sets `hasTrustDialogAccepted: true` to skip the trust prompt
/// 3. Sets `permissions.defaultMode: "acceptEdits"` to auto-accept file edits
/// 4. Writes the merged result to the worktree
///
/// This solves two issues:
/// - Issue 9: Eliminates the "Yes, proceed / No, exit" prompt on session start
/// - Issue 10: Enables auto-accept edits for seamless operation
fn create_worktree_settings(main_settings: &Path, worktree_settings: &Path) -> Result<()> {
    // Start with main repo settings or empty object
    let mut settings: Value = if main_settings.exists() {
        let content = std::fs::read_to_string(main_settings)
            .with_context(|| "Failed to read main repo settings.local.json")?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Ensure settings is an object
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.local.json must be a JSON object"))?;

    // Set hasTrustDialogAccepted to skip the trust prompt
    obj.insert("hasTrustDialogAccepted".to_string(), json!(true));

    // Ensure permissions object exists and set defaultMode to acceptEdits
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    permissions.insert("defaultMode".to_string(), json!("acceptEdits"));

    // Write the merged settings
    let content =
        serde_json::to_string_pretty(&settings).with_context(|| "Failed to serialize settings")?;
    std::fs::write(worktree_settings, content)
        .with_context(|| "Failed to write worktree settings.local.json")?;

    Ok(())
}

/// Configure hooks for a worktree with session context
///
/// This adds Claude Code hooks to the worktree's .claude/settings.json.
/// Hooks enable:
/// - Auto-handoff on PreCompact (context exhaustion)
/// - Learning protection via Stop hook
/// - Session lifecycle tracking
///
/// This should be called after worktree creation when session ID is known.
pub fn setup_worktree_hooks(
    worktree_path: &Path,
    stage_id: &str,
    session_id: &str,
    work_dir: &Path,
    hooks_dir: &Path,
) -> Result<()> {
    let config = HooksConfig::new(
        hooks_dir.to_path_buf(),
        stage_id.to_string(),
        session_id.to_string(),
        work_dir.to_path_buf(),
    );

    setup_hooks_for_worktree(worktree_path, &config)
        .with_context(|| format!("Failed to setup hooks for worktree: {}", worktree_path.display()))
}

/// Remove worktree-specific settings and symlinks
///
/// Called during worktree removal to clean up:
/// - .work symlink
/// - .claude directory (or legacy symlink)
/// - root CLAUDE.md symlink
pub fn cleanup_worktree_settings(worktree_path: &Path) {
    // Remove the .work symlink first to avoid issues
    let work_link = worktree_path.join(".work");
    if work_link.exists() || work_link.is_symlink() {
        std::fs::remove_file(&work_link).ok(); // Ignore errors
    }

    // Remove the .claude directory (it's a real directory now, not a symlink)
    let claude_dir = worktree_path.join(".claude");
    if claude_dir.exists() {
        std::fs::remove_dir_all(&claude_dir).ok(); // Ignore errors
    } else if claude_dir.is_symlink() {
        // Handle legacy symlink case
        std::fs::remove_file(&claude_dir).ok();
    }

    // Remove the root CLAUDE.md symlink
    let root_claude_md = worktree_path.join("CLAUDE.md");
    if root_claude_md.exists() || root_claude_md.is_symlink() {
        std::fs::remove_file(&root_claude_md).ok(); // Ignore errors
    }
}
