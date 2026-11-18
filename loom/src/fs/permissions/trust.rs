//! Trust management for Claude Code sessions that loom launches
//!
//! Claude Code stores per-project trust state in `~/.claude.json` under
//! `projects["/absolute/path"].hasTrustDialogAccepted`. This module manages
//! those entries so worktree sessions (`loom run`) and planning sessions (`loom plan`)
//! skip the "trust this folder?" prompt.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::fs::locking::locked_dir_update;

/// The invoking user's home directory
fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("Failed to determine home directory")
}

/// Claude Code's global state file inside `home`
fn state_file(home: &Path) -> PathBuf {
    home.join(".claude.json")
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

/// Replace ~/.claude.json with `settings` through a temp file and a rename
///
/// Live Claude Code processes read and write this file, so it is never truncated in place.
/// The temp file takes the target's mode (a 0600 file stays 0600); a new file keeps the
/// temp file's 0600. `atomic_write` is not used: it creates `<path>.tmp` with the umask
/// default mode, which would make a private file world-readable.
fn save_global_state(path: &Path, settings: &Value) -> Result<()> {
    let content =
        serde_json::to_string_pretty(settings).context("Failed to serialize global state")?;
    // A symlinked state file (a dotfile manager's) is replaced at its target, keeping the link.
    let target = match fs::canonicalize(path) {
        Ok(target) => target,
        Err(_) => path.to_path_buf(),
    };
    let parent = target
        .parent()
        .with_context(|| format!("{} has no parent directory", target.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("Failed to create a temporary file for {}", path.display()))?;
    if target.exists() {
        let permissions = fs::metadata(&target)
            .with_context(|| format!("Failed to read the mode of {}", path.display()))?
            .permissions();
        temp.as_file()
            .set_permissions(permissions)
            .with_context(|| format!("Failed to set the mode for {}", path.display()))?;
    }
    temp.write_all(content.as_bytes())
        .and_then(|()| temp.as_file().sync_all())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    temp.persist(&target)
        .with_context(|| format!("Failed to replace {}", target.display()))?;
    Ok(())
}

/// Read, edit and write back the state file in `home`; `edit` returns whether it changed anything
///
/// Claude Code does not take this lock: the rename in `save_global_state` keeps its reads whole,
/// and the lock only orders loom's own writers so none loses another's update.
fn update_global_state(home: &Path, edit: impl FnOnce(&mut Value) -> Result<bool>) -> Result<()> {
    let state_path = state_file(home);
    locked_dir_update(home, || {
        let mut settings = load_global_state(&state_path)?;
        if edit(&mut settings)? {
            save_global_state(&state_path, &settings)?;
        }
        Ok(())
    })
}

/// Add a worktree path to Claude Code's trusted projects
///
/// Sets `projects[worktree_path].hasTrustDialogAccepted = true` in
/// `~/.claude.json` so Claude Code skips the trust prompt when spawning
/// a session in the worktree.
pub fn trust_worktree(worktree_path: &Path) -> Result<()> {
    trust_project(&home_dir()?, worktree_path)
}

/// Record `project` as trusted in the `.claude.json` inside `home`
///
/// Sets `projects[<canonical project>].hasTrustDialogAccepted = true`. Claude Code runs no
/// hook, `SessionStart` included, while its folder-trust dialog is open, and only this
/// exact-path entry skips the dialog: a `--settings` key or a trusted parent directory does
/// not. `home` is the HOME the session's claude reads, which a caller may hold apart from the
/// invoking user's. An entry that is already `true` leaves the file untouched.
pub fn trust_project(home: &Path, project: &Path) -> Result<()> {
    // An unset HOME arrives as an empty path; joining it would write into the cwd.
    anyhow::ensure!(
        home.is_absolute(),
        "home directory '{}' is not an absolute path",
        home.display()
    );
    let canonical = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let key = canonical.to_string_lossy();
    update_global_state(home, |settings| set_trusted(settings, &key))
}

/// Set `projects[key].hasTrustDialogAccepted = true`; false when it already was
fn set_trusted(settings: &mut Value, key: &str) -> Result<bool> {
    let projects = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("~/.claude.json must be a JSON object"))?
        .entry("projects")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("projects must be a JSON object"))?;
    let accepted = projects
        .get(key)
        .and_then(|project| project.get("hasTrustDialogAccepted"))
        .and_then(Value::as_bool);
    if accepted == Some(true) {
        return Ok(false);
    }
    projects
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("project entry must be a JSON object"))?
        .insert("hasTrustDialogAccepted".to_string(), json!(true));
    Ok(true)
}

/// Remove a worktree path from Claude Code's trusted projects
///
/// Removes the `projects[worktree_path]` entry from `~/.claude.json`
/// when a worktree is deleted.
pub fn untrust_worktree(worktree_path: &Path) -> Result<()> {
    let home = home_dir()?;
    if !state_file(&home).exists() {
        return Ok(());
    }

    let canonical = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());
    let path_str = canonical.to_string_lossy();

    update_global_state(&home, |settings| {
        Ok(settings
            .as_object_mut()
            .and_then(|obj| obj.get_mut("projects"))
            .and_then(|p| p.as_object_mut())
            .map(|projects| projects.remove(path_str.as_ref()).is_some())
            .unwrap_or(false))
    })
}

/// Migrate from legacy `trustedDirectories` field to per-project trust
///
/// Called from `loom init` to clean up the old format. Removes any
/// `.worktrees/` entries from `trustedDirectories` since they had no effect.
pub fn migrate_legacy_trust(repo_root: &Path) -> Result<()> {
    let home = home_dir()?;
    if !state_file(&home).exists() {
        return Ok(());
    }

    let worktrees_dir = repo_root.join(".worktrees");
    let worktrees_path = worktrees_dir
        .canonicalize()
        .unwrap_or_else(|_| worktrees_dir.clone())
        .to_string_lossy()
        .to_string();

    update_global_state(&home, |settings| {
        Ok(settings
            .as_object_mut()
            .and_then(|obj| obj.get_mut("trustedDirectories"))
            .and_then(|td| td.as_array_mut())
            .map(|arr| {
                let before = arr.len();
                arr.retain(|v| v.as_str().map(|s| s != worktrees_path).unwrap_or(true));
                arr.len() != before
            })
            .unwrap_or(false))
    })
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
