//! Worktree cleanup operations

use anyhow::{Context, Result};
use std::path::Path;

use crate::git::runner::run_git_checked;

/// Clean up a single worktree for a stage
///
/// # Arguments
/// * `stage_id` - The stage ID whose worktree to remove
/// * `repo_root` - Path to the repository root
/// * `force` - Force removal even with uncommitted changes
///
/// # Returns
/// `true` if the worktree was removed, `false` if it didn't exist
pub fn cleanup_worktree(stage_id: &str, repo_root: &Path, force: bool) -> Result<bool> {
    crate::validation::validate_id(stage_id).context("Invalid stage ID for worktree cleanup")?;
    let worktree_path = repo_root.join(".worktrees").join(stage_id);

    if !worktree_directory_exists(&worktree_path)? {
        return Ok(false);
    }
    if !force {
        remove_worktree_scaffold(&worktree_path)?;
    }

    let worktree = worktree_path.to_string_lossy().to_string();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&worktree);

    match run_git_checked(&args, repo_root) {
        Ok(_) => Ok(true),
        Err(e) => {
            if force {
                std::fs::remove_dir_all(&worktree_path).with_context(|| {
                    format!(
                        "Failed to manually remove worktree at {} after git error: {}",
                        worktree_path.display(),
                        e
                    )
                })?;
                Ok(true)
            } else {
                Err(e).with_context(|| {
                    format!(
                        "git worktree remove refused for {} and force was not set; \
                         not destroying uncommitted files.",
                        worktree_path.display()
                    )
                })
            }
        }
    }
}

/// Strictly inspect a worktree path without following a symlink outside the repository.
pub(crate) fn worktree_directory_exists(worktree_path: &Path) -> Result<bool> {
    let metadata = match std::fs::symlink_metadata(worktree_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to inspect {}", worktree_path.display()))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!(
            "Refusing to treat non-directory path {} as a worktree",
            worktree_path.display()
        );
    }
    Ok(true)
}

/// Remove only Loom-generated scaffold before non-forced Git removal.
pub(crate) fn remove_worktree_scaffold(worktree_path: &Path) -> Result<()> {
    remove_required_symlink(&worktree_path.join(".work"))?;
    let claude_dir = worktree_path.join(".claude");
    if claude_dir.is_symlink() {
        remove_if_symlink(&claude_dir)?;
    } else if claude_dir.exists() {
        remove_known_claude_scaffold(&claude_dir)?;
    }
    remove_required_symlink(&worktree_path.join("CLAUDE.md"))?;
    Ok(())
}

fn remove_known_claude_scaffold(claude_dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(claude_dir)
        .with_context(|| format!("Failed to inspect {}", claude_dir.display()))?
    {
        let entry = entry.context("Failed to inspect .claude scaffold entry")?;
        let name = entry.file_name();
        let path = entry.path();
        match name.to_str() {
            Some("CLAUDE.md") => remove_required_symlink(&path)?,
            Some("settings.json" | "settings.local.json") => {
                if path.is_dir() && !path.is_symlink() {
                    anyhow::bail!("Refusing to remove unexpected directory {}", path.display());
                }
                std::fs::remove_file(&path).with_context(|| {
                    format!("Failed to remove generated scaffold {}", path.display())
                })?;
            }
            _ => anyhow::bail!(
                "Refusing to remove worktree: {} contains non-scaffold entry '{}'",
                claude_dir.display(),
                name.to_string_lossy()
            ),
        }
    }
    std::fs::remove_dir(claude_dir).with_context(|| {
        format!(
            "Failed to remove scaffold directory {}",
            claude_dir.display()
        )
    })
}

fn remove_required_symlink(path: &Path) -> Result<()> {
    if path.exists() && !path.is_symlink() {
        anyhow::bail!(
            "Refusing to remove non-symlink worktree scaffold at {}",
            path.display()
        );
    }
    remove_if_symlink(path)
}

fn remove_if_symlink(path: &Path) -> Result<()> {
    if path.is_symlink() {
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove symlink at {}", path.display()))?;
    }
    Ok(())
}
