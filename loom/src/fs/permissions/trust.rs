//! Trust management for loom worktrees
//!
//! Claude Code stores per-project trust state in `~/.claude.json` under
//! `projects["/absolute/path"].hasTrustDialogAccepted`. This module manages
//! those entries so worktree sessions skip the "trust this folder?" prompt.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Path to Claude Code's global state file
fn global_state_path() -> Result<std::path::PathBuf> {
    let home_dir = dirs::home_dir().context("Failed to determine home directory")?;
    Ok(home_dir.join(".claude.json"))
}

/// Load and parse ~/.claude.json, returning empty object if missing
fn load_global_state(path: &Path) -> Result<Value> {
    if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {} as JSON", path.display()))
    } else {
        Ok(json!({}))
    }
}

/// Write ~/.claude.json back to disk
fn save_global_state(path: &Path, settings: &Value) -> Result<()> {
    let content =
        serde_json::to_string_pretty(settings).context("Failed to serialize global state")?;
    fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))
}

/// Add a worktree path to Claude Code's trusted projects
///
/// Sets `projects[worktree_path].hasTrustDialogAccepted = true` in
/// `~/.claude.json` so Claude Code skips the trust prompt when spawning
/// a session in the worktree.
pub fn trust_worktree(worktree_path: &Path) -> Result<()> {
    let state_path = global_state_path()?;

    let canonical = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());
    let path_str = canonical.to_string_lossy().to_string();

    let mut settings = load_global_state(&state_path)?;

    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("~/.claude.json must be a JSON object"))?;

    let projects = settings_obj.entry("projects").or_insert_with(|| json!({}));

    let projects_obj = projects
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("projects must be a JSON object"))?;

    // Check if already trusted
    if let Some(project) = projects_obj.get(&path_str) {
        if project
            .get("hasTrustDialogAccepted")
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            return Ok(());
        }
    }

    // Set trust for this path
    let project = projects_obj.entry(&path_str).or_insert_with(|| json!({}));

    let project_obj = project
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("project entry must be a JSON object"))?;

    project_obj.insert("hasTrustDialogAccepted".to_string(), json!(true));

    save_global_state(&state_path, &settings)?;

    Ok(())
}

/// Remove a worktree path from Claude Code's trusted projects
///
/// Removes the `projects[worktree_path]` entry from `~/.claude.json`
/// when a worktree is deleted.
pub fn untrust_worktree(worktree_path: &Path) -> Result<()> {
    let state_path = global_state_path()?;

    if !state_path.exists() {
        return Ok(());
    }

    let canonical = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());
    let path_str = canonical.to_string_lossy().to_string();

    let mut settings = load_global_state(&state_path)?;

    let modified = settings
        .as_object_mut()
        .and_then(|obj| obj.get_mut("projects"))
        .and_then(|p| p.as_object_mut())
        .map(|projects| projects.remove(&path_str).is_some())
        .unwrap_or(false);

    if modified {
        save_global_state(&state_path, &settings)?;
    }

    Ok(())
}

/// Migrate from legacy `trustedDirectories` field to per-project trust
///
/// Called from `loom init` to clean up the old format. Removes any
/// `.worktrees/` entries from `trustedDirectories` since they had no effect.
pub fn migrate_legacy_trust(repo_root: &Path) -> Result<()> {
    let state_path = global_state_path()?;

    if !state_path.exists() {
        return Ok(());
    }

    let worktrees_dir = repo_root.join(".worktrees");
    let worktrees_path = worktrees_dir
        .canonicalize()
        .unwrap_or_else(|_| worktrees_dir.clone())
        .to_string_lossy()
        .to_string();

    let mut settings = load_global_state(&state_path)?;

    let modified = settings
        .as_object_mut()
        .and_then(|obj| obj.get_mut("trustedDirectories"))
        .and_then(|td| td.as_array_mut())
        .map(|arr| {
            let before = arr.len();
            arr.retain(|v| v.as_str().map(|s| s != worktrees_path).unwrap_or(true));
            arr.len() != before
        })
        .unwrap_or(false);

    if modified {
        save_global_state(&state_path, &settings)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn with_test_home(test_fn: impl FnOnce(&Path, &Path)) {
        let home = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home.path());
        test_fn(home.path(), worktree.path());
        // Restore HOME
        if let Some(h) = old_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    #[serial]
    fn test_trust_worktree_creates_entry() {
        with_test_home(|home, worktree| {
            trust_worktree(worktree).unwrap();

            let state_path = home.join(".claude.json");
            let content = fs::read_to_string(&state_path).unwrap();
            let settings: Value = serde_json::from_str(&content).unwrap();

            let path_str = worktree
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(
                settings["projects"][&path_str]["hasTrustDialogAccepted"],
                true
            );
        });
    }

    #[test]
    #[serial]
    fn test_trust_worktree_idempotent() {
        with_test_home(|home, worktree| {
            trust_worktree(worktree).unwrap();
            trust_worktree(worktree).unwrap();

            let state_path = home.join(".claude.json");
            let content = fs::read_to_string(&state_path).unwrap();
            let settings: Value = serde_json::from_str(&content).unwrap();

            let projects = settings["projects"].as_object().unwrap();
            assert_eq!(projects.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn test_untrust_worktree_removes_entry() {
        with_test_home(|home, worktree| {
            trust_worktree(worktree).unwrap();
            untrust_worktree(worktree).unwrap();

            let state_path = home.join(".claude.json");
            let content = fs::read_to_string(&state_path).unwrap();
            let settings: Value = serde_json::from_str(&content).unwrap();

            let projects = settings["projects"].as_object().unwrap();
            assert!(projects.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_untrust_worktree_noop_if_missing() {
        with_test_home(|_home, worktree| {
            // Should not error even if no state file exists
            untrust_worktree(worktree).unwrap();
        });
    }

    #[test]
    #[serial]
    fn test_trust_preserves_existing_data() {
        with_test_home(|home, worktree| {
            let state_path = home.join(".claude.json");
            let existing = json!({
                "projects": {
                    "/some/other/project": {
                        "hasTrustDialogAccepted": true,
                        "allowedTools": ["Bash"]
                    }
                },
                "otherField": 42
            });
            fs::write(
                &state_path,
                serde_json::to_string_pretty(&existing).unwrap(),
            )
            .unwrap();

            trust_worktree(worktree).unwrap();

            let content = fs::read_to_string(&state_path).unwrap();
            let settings: Value = serde_json::from_str(&content).unwrap();

            // Existing project preserved
            assert_eq!(
                settings["projects"]["/some/other/project"]["hasTrustDialogAccepted"],
                true
            );
            assert_eq!(
                settings["projects"]["/some/other/project"]["allowedTools"][0],
                "Bash"
            );
            // Other fields preserved
            assert_eq!(settings["otherField"], 42);
            // New entry added
            let path_str = worktree
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(
                settings["projects"][&path_str]["hasTrustDialogAccepted"],
                true
            );
        });
    }

    #[test]
    #[serial]
    fn test_migrate_legacy_trust() {
        with_test_home(|home, _worktree| {
            let repo = TempDir::new().unwrap();
            let worktrees_dir = repo.path().join(".worktrees");
            fs::create_dir_all(&worktrees_dir).unwrap();

            let worktrees_path = worktrees_dir
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string();

            let state_path = home.join(".claude.json");
            let existing = json!({
                "trustedDirectories": [worktrees_path, "/other/dir"],
                "projects": {}
            });
            fs::write(
                &state_path,
                serde_json::to_string_pretty(&existing).unwrap(),
            )
            .unwrap();

            migrate_legacy_trust(repo.path()).unwrap();

            let content = fs::read_to_string(&state_path).unwrap();
            let settings: Value = serde_json::from_str(&content).unwrap();

            let trusted = settings["trustedDirectories"].as_array().unwrap();
            assert_eq!(trusted.len(), 1);
            assert_eq!(trusted[0], "/other/dir");
        });
    }
}
