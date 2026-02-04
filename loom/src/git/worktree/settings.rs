//! Worktree settings management
//!
//! Handles creation of settings files (.claude/, CLAUDE.md) for worktrees.
//! Also supports hooks configuration when session context is available.

use anyhow::{Context, Result};
#[allow(unused_imports)] // Required for lock_shared() method on File
use fs2::FileExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::hooks::{setup_hooks_for_worktree, HooksConfig};

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
/// We create a real directory and symlink CLAUDE.md from main repo.
/// settings.json is created separately by the hooks system with merged global + session hooks.
/// This ensures:
/// 1. Instructions (CLAUDE.md) are shared
/// 2. Permissions (settings.json) include both global hooks and session-specific hooks
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

        // Create settings.json with trust and auto-accept settings merged with main repo settings
        let main_settings = main_claude_dir.join("settings.json");
        let worktree_settings = worktree_claude_dir.join("settings.json");
        create_worktree_settings(&main_settings, &worktree_settings)?;

        // Copy settings.local.json if it exists (contains user-granted runtime permissions)
        // Use file locking to prevent reading a partially written file during concurrent syncs
        let main_settings_local = main_claude_dir.join("settings.local.json");
        let worktree_settings_local = worktree_claude_dir.join("settings.local.json");
        if main_settings_local.exists() {
            copy_file_with_shared_lock(&main_settings_local, &worktree_settings_local)
                .with_context(|| "Failed to copy settings.local.json to worktree")?;
        }
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

/// Copy a file with a shared (read) lock on the source.
///
/// This prevents reading a partially written file during concurrent writes.
/// The source file is locked with a shared lock (allowing other readers),
/// and the content is read and written to the destination atomically.
fn copy_file_with_shared_lock(src: &Path, dst: &Path) -> Result<()> {
    // Open the source file and acquire a shared lock
    let src_file = File::open(src)
        .with_context(|| format!("Failed to open source file: {}", src.display()))?;

    src_file
        .lock_shared()
        .with_context(|| format!("Failed to acquire shared lock on: {}", src.display()))?;

    // Read content while holding the lock
    let mut content = Vec::new();
    let mut reader = &src_file;
    reader
        .read_to_end(&mut content)
        .with_context(|| format!("Failed to read source file: {}", src.display()))?;

    // Lock is released when src_file is dropped, but we can write to dst now
    // since we have the complete content

    // Write to destination
    let mut dst_file = File::create(dst)
        .with_context(|| format!("Failed to create destination file: {}", dst.display()))?;

    dst_file
        .write_all(&content)
        .with_context(|| format!("Failed to write to destination file: {}", dst.display()))?;

    Ok(())
}

/// Merge permissions from main repo's settings.local.json into a worktree.
///
/// This is the public interface for refreshing permissions in a worktree.
/// Instead of overwriting, it merges permissions from both sources:
/// - Permissions from the main repo's settings.local.json
/// - Existing permissions in the worktree's settings.local.json (if any)
///
/// This ensures worktree-specific permissions are preserved while still
/// receiving updates from the main repo.
pub fn refresh_worktree_settings_local(worktree_path: &Path, repo_root: &Path) -> Result<bool> {
    let main_settings_local = repo_root.join(".claude/settings.local.json");
    let worktree_settings_local = worktree_path.join(".claude/settings.local.json");

    if !main_settings_local.exists() {
        return Ok(false);
    }

    // Ensure .claude directory exists in worktree
    let worktree_claude_dir = worktree_path.join(".claude");
    if !worktree_claude_dir.exists() {
        std::fs::create_dir_all(&worktree_claude_dir)
            .with_context(|| "Failed to create .claude directory in worktree")?;
    }

    // Read main repo settings with shared lock
    let main_settings = read_settings_with_shared_lock(&main_settings_local)?;

    // Read existing worktree settings (if any)
    let worktree_settings = if worktree_settings_local.exists() {
        read_settings(&worktree_settings_local)?
    } else {
        json!({})
    };

    // Extract permissions from both
    let (main_allow, main_deny) = extract_permissions(&main_settings);
    let (wt_allow, wt_deny) = extract_permissions(&worktree_settings);

    // Merge permissions (union with deduplication)
    let merged_allow = merge_permission_vecs(main_allow, wt_allow);
    let merged_deny = merge_permission_vecs(main_deny, wt_deny);

    // Build merged settings (start with main settings as base)
    let mut merged = main_settings.clone();
    set_permissions(&mut merged, merged_allow, merged_deny)?;

    // Write merged result
    let content =
        serde_json::to_string_pretty(&merged).with_context(|| "Failed to serialize settings")?;
    std::fs::write(&worktree_settings_local, content)
        .with_context(|| format!("Failed to write {}", worktree_settings_local.display()))?;

    Ok(true)
}

/// Read and parse a settings.json file with a shared lock
fn read_settings_with_shared_lock(path: &Path) -> Result<Value> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;

    file.lock_shared()
        .with_context(|| format!("Failed to acquire shared lock on {}", path.display()))?;

    let mut content = String::new();
    let mut reader = &file;
    reader
        .read_to_string(&mut content)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    // Lock released when file is dropped
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {} as JSON", path.display()))
}

/// Read and parse a settings.json file
fn read_settings(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {} as JSON", path.display()))
}

/// Extract allow and deny permission arrays from settings
fn extract_permissions(settings: &Value) -> (Vec<String>, Vec<String>) {
    let permissions = settings.get("permissions");

    let allow = permissions
        .and_then(|p| p.get("allow"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let deny = permissions
        .and_then(|p| p.get("deny"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    (allow, deny)
}

/// Merge two permission vectors, removing duplicates
fn merge_permission_vecs(a: Vec<String>, b: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result = Vec::new();

    for perm in a.into_iter().chain(b.into_iter()) {
        if seen.insert(perm.clone()) {
            result.push(perm);
        }
    }

    result
}

/// Set permissions in a settings Value
fn set_permissions(settings: &mut Value, allow: Vec<String>, deny: Vec<String>) -> Result<()> {
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Settings must be a JSON object"))?;

    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    if !allow.is_empty() {
        permissions.insert("allow".to_string(), json!(allow));
    }
    if !deny.is_empty() {
        permissions.insert("deny".to_string(), json!(deny));
    }

    Ok(())
}

/// Create settings.json for a worktree with trust and auto-accept settings.
///
/// This function:
/// 1. Reads the main repo's settings.json (if it exists)
/// 2. Sets `hasTrustDialogAccepted: true` to skip the trust prompt
/// 3. Sets `permissions.defaultMode: "acceptEdits"` to auto-accept file edits
/// 4. Writes the merged result to the worktree
///
/// Note: This creates the base settings.json. The hooks system will later merge in
/// session-specific hooks via setup_worktree_hooks().
///
/// This solves two issues:
/// - Issue 9: Eliminates the "Yes, proceed / No, exit" prompt on session start
/// - Issue 10: Enables auto-accept edits for seamless operation
fn create_worktree_settings(main_settings: &Path, worktree_settings: &Path) -> Result<()> {
    // Start with main repo settings or empty object
    let mut settings: Value = if main_settings.exists() {
        let content = std::fs::read_to_string(main_settings)
            .with_context(|| "Failed to read main repo settings.json")?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Ensure settings is an object
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json must be a JSON object"))?;

    // Set hasTrustDialogAccepted to skip the trust prompt
    obj.insert("hasTrustDialogAccepted".to_string(), json!(true));

    // Ensure permissions object exists and set defaultMode to acceptEdits
    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    permissions.insert("defaultMode".to_string(), json!("acceptEdits"));

    // Remove any stale LOOM_MAIN_AGENT_PID from copied settings
    // This variable must be set dynamically by the wrapper script at runtime
    if let Some(env) = obj.get_mut("env").and_then(|v| v.as_object_mut()) {
        env.remove("LOOM_MAIN_AGENT_PID");
    }

    // Write the merged settings
    let content =
        serde_json::to_string_pretty(&settings).with_context(|| "Failed to serialize settings")?;
    std::fs::write(worktree_settings, content)
        .with_context(|| "Failed to write worktree settings.json")?;

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
    // Canonicalize work_dir to absolute path so hooks work regardless of
    // Claude Code's current working directory. This fixes "spawn /bin/sh ENOENT"
    // errors that occur when hooks run from a deleted/changed directory.
    let absolute_work_dir = work_dir
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(work_dir));

    let config = HooksConfig::new(
        hooks_dir.to_path_buf(),
        stage_id.to_string(),
        session_id.to_string(),
        absolute_work_dir,
    );

    setup_hooks_for_worktree(worktree_path, &config).with_context(|| {
        format!(
            "Failed to setup hooks for worktree: {}",
            worktree_path.display()
        )
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_permissions() {
        let settings = json!({
            "permissions": {
                "allow": ["Read(foo)", "Write(bar)"],
                "deny": ["Bash(rm:*)"]
            }
        });

        let (allow, deny) = extract_permissions(&settings);
        assert_eq!(allow, vec!["Read(foo)", "Write(bar)"]);
        assert_eq!(deny, vec!["Bash(rm:*)"]);
    }

    #[test]
    fn test_extract_permissions_empty() {
        let settings = json!({});
        let (allow, deny) = extract_permissions(&settings);
        assert!(allow.is_empty());
        assert!(deny.is_empty());
    }

    #[test]
    fn test_merge_permission_vecs() {
        let a = vec!["Read(foo)".to_string(), "Write(bar)".to_string()];
        let b = vec!["Write(bar)".to_string(), "Bash(cargo:*)".to_string()];

        let merged = merge_permission_vecs(a, b);
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&"Read(foo)".to_string()));
        assert!(merged.contains(&"Write(bar)".to_string()));
        assert!(merged.contains(&"Bash(cargo:*)".to_string()));
    }

    #[test]
    fn test_merge_permission_vecs_empty() {
        let a: Vec<String> = vec![];
        let b = vec!["Read(foo)".to_string()];

        let merged = merge_permission_vecs(a, b);
        assert_eq!(merged, vec!["Read(foo)"]);
    }

    #[test]
    fn test_refresh_worktree_settings_local_merges_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path().join("repo");
        let worktree = temp_dir.path().join("worktree");

        // Setup main repo with permission A
        let main_claude = repo_root.join(".claude");
        std::fs::create_dir_all(&main_claude).unwrap();
        let main_settings = json!({
            "permissions": {
                "allow": ["Read(main_perm)"]
            }
        });
        std::fs::write(
            main_claude.join("settings.local.json"),
            serde_json::to_string_pretty(&main_settings).unwrap(),
        )
        .unwrap();

        // Setup worktree with permission B
        let wt_claude = worktree.join(".claude");
        std::fs::create_dir_all(&wt_claude).unwrap();
        let wt_settings = json!({
            "permissions": {
                "allow": ["Write(worktree_perm)"]
            }
        });
        std::fs::write(
            wt_claude.join("settings.local.json"),
            serde_json::to_string_pretty(&wt_settings).unwrap(),
        )
        .unwrap();

        // Refresh should merge, not overwrite
        let result = refresh_worktree_settings_local(&worktree, &repo_root).unwrap();
        assert!(result);

        // Verify merged result
        let merged_content =
            std::fs::read_to_string(wt_claude.join("settings.local.json")).unwrap();
        let merged: Value = serde_json::from_str(&merged_content).unwrap();

        let (allow, _deny) = extract_permissions(&merged);
        assert!(allow.contains(&"Read(main_perm)".to_string()));
        assert!(allow.contains(&"Write(worktree_perm)".to_string()));
    }

    #[test]
    fn test_refresh_worktree_settings_local_no_existing_worktree_settings() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path().join("repo");
        let worktree = temp_dir.path().join("worktree");

        // Setup main repo with permission
        let main_claude = repo_root.join(".claude");
        std::fs::create_dir_all(&main_claude).unwrap();
        let main_settings = json!({
            "permissions": {
                "allow": ["Read(main_perm)"],
                "deny": ["Bash(rm:*)"]
            }
        });
        std::fs::write(
            main_claude.join("settings.local.json"),
            serde_json::to_string_pretty(&main_settings).unwrap(),
        )
        .unwrap();

        // Worktree has no existing settings
        std::fs::create_dir_all(&worktree).unwrap();

        // Refresh should create new settings
        let result = refresh_worktree_settings_local(&worktree, &repo_root).unwrap();
        assert!(result);

        // Verify result contains main permissions
        let wt_settings_path = worktree.join(".claude/settings.local.json");
        assert!(wt_settings_path.exists());

        let content = std::fs::read_to_string(&wt_settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        let (allow, deny) = extract_permissions(&settings);
        assert_eq!(allow, vec!["Read(main_perm)"]);
        assert_eq!(deny, vec!["Bash(rm:*)"]);
    }

    #[test]
    fn test_refresh_worktree_settings_local_no_main_settings() {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path().join("repo");
        let worktree = temp_dir.path().join("worktree");

        // Setup repo without settings.local.json
        let main_claude = repo_root.join(".claude");
        std::fs::create_dir_all(&main_claude).unwrap();

        std::fs::create_dir_all(&worktree).unwrap();

        // Should return false when no main settings exist
        let result = refresh_worktree_settings_local(&worktree, &repo_root).unwrap();
        assert!(!result);
    }
}
