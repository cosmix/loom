//! Permission synchronization from worktree to main repo settings
//!
//! When Claude Code sessions run in worktrees, they may be granted additional
//! permissions that are stored in the worktree's settings.local.json. This module
//! provides functionality to sync those permissions back to the main repo's
//! settings.local.json file, filtering out worktree-specific paths.

use anyhow::{Context, Result};
use fs2::FileExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::git::worktree::refresh_worktree_settings_local;

/// Patterns that indicate a worktree-specific permission that should not be synced
const WORKTREE_PATH_PATTERNS: &[&str] = &["../../", ".worktrees/"];

/// Sync permissions from a worktree's settings.local.json to the main repo's settings
///
/// This function:
/// 1. Reads the worktree's settings.local.json
/// 2. Extracts permissions.allow and permissions.deny arrays
/// 3. Filters out worktree-specific paths (containing ../../ or .worktrees/)
/// 4. Acquires an exclusive file lock on the main settings.local.json
/// 5. Merges new permissions (skipping duplicates)
/// 6. Writes back atomically
/// 7. Propagates the updated permissions to all other existing worktrees
pub fn sync_worktree_permissions(
    worktree_path: &Path,
    main_repo_path: &Path,
) -> Result<SyncResult> {
    let worktree_settings_path = worktree_path.join(".claude/settings.local.json");
    let main_settings_path = main_repo_path.join(".claude/settings.local.json");

    // Read worktree settings
    let worktree_settings = read_settings(&worktree_settings_path)?;

    // Extract permissions from worktree settings
    let (allow_perms, deny_perms) = extract_permissions(&worktree_settings);

    // Filter out worktree-specific paths
    let filtered_allow: Vec<String> = allow_perms
        .into_iter()
        .filter(|p| !is_worktree_specific_permission(p))
        .collect();

    let filtered_deny: Vec<String> = deny_perms
        .into_iter()
        .filter(|p| !is_worktree_specific_permission(p))
        .collect();

    // If nothing to sync, return early
    if filtered_allow.is_empty() && filtered_deny.is_empty() {
        return Ok(SyncResult {
            allow_added: 0,
            deny_added: 0,
            worktrees_updated: 0,
        });
    }

    // Acquire exclusive lock and merge permissions
    let mut result =
        merge_permissions_with_lock(&main_settings_path, &filtered_allow, &filtered_deny)?;

    // Propagate updated permissions to all other worktrees
    // This ensures all worktrees have the latest permissions after a sync
    if result.allow_added > 0 || result.deny_added > 0 {
        result.worktrees_updated =
            propagate_permissions_to_worktrees(main_repo_path, Some(worktree_path))?;
    }

    Ok(result)
}

/// Result of a permission sync operation
#[derive(Debug, Default)]
pub struct SyncResult {
    /// Number of allow permissions added
    pub allow_added: usize,
    /// Number of deny permissions added
    pub deny_added: usize,
    /// Number of worktrees updated with propagated permissions
    pub worktrees_updated: usize,
}

/// Read and parse a settings.json file
fn read_settings(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

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

/// Check if a permission string contains worktree-specific path patterns
fn is_worktree_specific_permission(permission: &str) -> bool {
    WORKTREE_PATH_PATTERNS
        .iter()
        .any(|pattern| permission.contains(pattern))
}

/// Merge permissions into the main settings file with exclusive file locking
fn merge_permissions_with_lock(
    main_settings_path: &Path,
    allow_perms: &[String],
    deny_perms: &[String],
) -> Result<SyncResult> {
    // Ensure .claude directory exists
    if let Some(parent) = main_settings_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Open or create the file with read/write access
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(main_settings_path)
        .with_context(|| format!("Failed to open {}", main_settings_path.display()))?;

    // Acquire exclusive lock
    file.lock_exclusive()
        .with_context(|| format!("Failed to lock {}", main_settings_path.display()))?;

    // Read current content
    let mut content = String::new();
    let mut file_reader = &file;
    file_reader.read_to_string(&mut content).ok();

    // Parse or create settings
    let mut settings: Value = if content.is_empty() {
        json!({})
    } else {
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", main_settings_path.display()))?
    };

    // Merge permissions
    let result = merge_permission_arrays(&mut settings, allow_perms, deny_perms)?;

    // Write back directly to the locked file handle
    let new_content =
        serde_json::to_string_pretty(&settings).context("Failed to serialize settings")?;

    // Truncate and rewrite using the locked file handle (preserves lock)
    let mut file = file; // rebind as mutable
    file.set_len(0)
        .with_context(|| format!("Failed to truncate {}", main_settings_path.display()))?;
    file.seek(std::io::SeekFrom::Start(0))
        .with_context(|| format!("Failed to seek in {}", main_settings_path.display()))?;
    file.write_all(new_content.as_bytes())
        .with_context(|| format!("Failed to write {}", main_settings_path.display()))?;
    file.flush()
        .with_context(|| format!("Failed to flush {}", main_settings_path.display()))?;

    // Lock is released when file is dropped
    drop(file);

    Ok(result)
}

/// Merge permission arrays into settings, avoiding duplicates
fn merge_permission_arrays(
    settings: &mut Value,
    allow_perms: &[String],
    deny_perms: &[String],
) -> Result<SyncResult> {
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Settings must be a JSON object"))?;

    let permissions = settings_obj
        .entry("permissions")
        .or_insert_with(|| json!({}));

    let permissions_obj = permissions
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    // Merge allow permissions
    let allow_added = merge_permission_array(permissions_obj, "allow", allow_perms)?;

    // Merge deny permissions
    let deny_added = merge_permission_array(permissions_obj, "deny", deny_perms)?;

    Ok(SyncResult {
        allow_added,
        deny_added,
        worktrees_updated: 0, // Set by caller after propagation
    })
}

/// Merge a single permission array (allow or deny), returning count of added permissions
fn merge_permission_array(
    permissions_obj: &mut serde_json::Map<String, Value>,
    key: &str,
    new_perms: &[String],
) -> Result<usize> {
    if new_perms.is_empty() {
        return Ok(0);
    }

    let arr = permissions_obj.entry(key).or_insert_with(|| json!([]));

    let arr_vec = arr
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions.{key} must be a JSON array"))?;

    // Collect existing permissions for deduplication
    let existing: HashSet<String> = arr_vec
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // Add new permissions that don't already exist
    let mut added = 0;
    for perm in new_perms {
        if !existing.contains(perm) {
            arr_vec.push(json!(perm));
            added += 1;
        }
    }

    Ok(added)
}

/// List all worktree directories in .worktrees/
///
/// Returns paths to worktree directories, excluding the optional source worktree.
fn list_worktree_paths(main_repo_path: &Path, exclude: Option<&Path>) -> Result<Vec<PathBuf>> {
    let worktrees_dir = main_repo_path.join(".worktrees");

    if !worktrees_dir.exists() {
        return Ok(Vec::new());
    }

    let mut worktree_paths = Vec::new();

    let entries = fs::read_dir(&worktrees_dir).with_context(|| {
        format!(
            "Failed to read worktrees directory: {}",
            worktrees_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip if not a directory
        if !path.is_dir() {
            continue;
        }

        // Skip the source worktree (the one that triggered the sync)
        if let Some(exclude_path) = exclude {
            // Canonicalize both paths for comparison, or fall back to comparing as-is
            let path_canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            let exclude_canonical = exclude_path
                .canonicalize()
                .unwrap_or_else(|_| exclude_path.to_path_buf());
            if path_canonical == exclude_canonical {
                continue;
            }
        }

        // Check if this is a valid worktree (has .claude directory)
        let claude_dir = path.join(".claude");
        if claude_dir.exists() {
            worktree_paths.push(path);
        }
    }

    Ok(worktree_paths)
}

/// Propagate permissions to all existing worktrees
///
/// After permissions are synced to the main repo, this function copies the updated
/// settings.local.json to all other worktrees. This ensures all worktrees have
/// the latest permissions without requiring a restart.
///
/// # Arguments
/// * `main_repo_path` - Path to the main repository root
/// * `exclude` - Optional path to exclude (typically the worktree that triggered the sync)
///
/// # Returns
/// Number of worktrees updated
fn propagate_permissions_to_worktrees(
    main_repo_path: &Path,
    exclude: Option<&Path>,
) -> Result<usize> {
    let worktree_paths = list_worktree_paths(main_repo_path, exclude)?;

    let mut updated = 0;
    for worktree_path in worktree_paths {
        // Use refresh_worktree_settings_local which handles locking
        match refresh_worktree_settings_local(&worktree_path, main_repo_path) {
            Ok(true) => updated += 1,
            Ok(false) => {} // No main settings.local.json exists, nothing to propagate
            Err(e) => {
                // Log warning but continue with other worktrees
                eprintln!(
                    "Warning: Failed to propagate permissions to {}: {}",
                    worktree_path.display(),
                    e
                );
            }
        }
    }

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_worktree_specific_permission() {
        assert!(is_worktree_specific_permission("Read(../../.work/**)"));
        assert!(is_worktree_specific_permission(
            "Write(.worktrees/stage-1/**)"
        ));
        assert!(!is_worktree_specific_permission("Read(.work/**)"));
        assert!(!is_worktree_specific_permission("Bash(cargo:*)"));
    }

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
}
