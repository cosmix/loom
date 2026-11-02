//! Failure handling for non-forced `git worktree remove`: the operator-facing
//! error text and recovery of a worktree git already half-removed.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::git::worktree::WorktreeGit;

/// Cap on how many paths an error lists verbatim. The message is persisted
/// into stage frontmatter and shipped in every status frame (2 MiB IPC cap),
/// so it must be bounded.
const MAX_BLOCKING_PATHS: usize = 20;

/// Empty directories left by the worktree scaffold or its runtime writers.
const SCAFFOLD_DIRS: [&str; 4] = [".claude", ".claude/.cc-writes", ".loom", ".loom/cache"];

/// Git metadata is reconciled, but filesystem ownership remains unproved.
#[derive(Debug)]
pub(super) struct RetainedScaffold(PathBuf);

impl std::fmt::Display for RetainedScaffold {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Git worktree registry reconciled for {}, but directories remain; \
             their ownership cannot be established, so the retained path was not deleted or moved. \
             Inspect the retained path and use explicit forced cleanup to remove it",
            self.0.display()
        )
    }
}

impl std::error::Error for RetainedScaffold {}

/// Build the failure error for a non-forced removal. Only git's dirty-tree
/// refusal is described as a refusal; any other failure carries git's own
/// stderr (the underlying error) as the reason.
pub(super) fn removal_failure(
    worktree_path: &Path,
    repo_root: &Path,
    e: anyhow::Error,
) -> anyhow::Error {
    let refused = format!("{e:#}").contains("contains modified or untracked files");
    let mut message = if refused {
        format!(
            "git worktree remove refused for {} and force was not set; \
             leaving the worktree in place (git's reason is the underlying error).",
            worktree_path.display()
        )
    } else {
        format!(
            "git worktree remove failed for {}; leaving the worktree in place \
             (git's reason is the underlying error).",
            worktree_path.display()
        )
    };
    let blocking = blocking_paths(worktree_path, repo_root);
    if !blocking.is_empty() {
        message.push_str(&format!("\n{blocking}"));
    }
    e.context(message)
}

/// Whether the error and missing checkout pointer suggest half-removal.
/// The surviving administrative registry is checked separately before recovery.
fn half_removed(worktree_path: &Path, e: &anyhow::Error) -> bool {
    let text = format!("{e:#}");
    (text.contains("Directory not empty") || text.contains("is not a working tree"))
        && matches!(
            std::fs::symlink_metadata(worktree_path.join(".git")),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound
        )
}

/// Recover a worktree git half-removed (tracked files and admin dir gone,
/// directory left behind by a late write from the stage's session).
///
/// Returns `Ok(true)` when the directory and registry entry are already gone;
/// `Ok(false)` when the failure is not that state. Surviving directories
/// require explicit forced cleanup: their names cannot prove ownership once
/// Git's admin directory is gone. Even a pinned descriptor cannot make POSIX
/// `rmdir` conditional on inode identity, and quarantine only moves that race.
pub(super) fn recover_half_removed(
    worktree_path: &Path,
    repo_root: &Path,
    e: &anyhow::Error,
) -> Result<bool> {
    if !half_removed(worktree_path, e) {
        return Ok(false);
    }
    require_unregistered(worktree_path, repo_root)?;
    match std::fs::symlink_metadata(worktree_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(true);
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let unknown = unknown_entries(worktree_path)?;
    if !unknown.is_empty() {
        anyhow::bail!(
            "git removed worktree {} but files that are not loom scaffold remain; \
             nothing was deleted. Remaining files:\n{}",
            worktree_path.display(),
            cap_lines(&unknown)
        );
    }
    Err(RetainedScaffold(worktree_path.to_path_buf()).into())
}

/// A missing checkout pointer does not prove Git's administrative HEAD/index
/// are gone. Never prune a surviving registration to make recovery appear done.
pub(super) fn require_unregistered(worktree: &Path, repo_root: &Path) -> Result<()> {
    if let Some(registered) = registered_worktree(worktree, repo_root)? {
        anyhow::bail!(
            "Git still retains the registration for {} at HEAD {}; \
             preserving its administrative refs and index. Repair the worktree's \
             .git linkage and verify its retained work before cleanup",
            worktree.display(),
            registered.head
        );
    }
    Ok(())
}

pub(super) fn registered_worktree(
    worktree: &Path,
    repo_root: &Path,
) -> Result<Option<crate::git::worktree::WorktreeInfo>> {
    let expected = canonical_parent_path(worktree)?;
    for registered in crate::git::worktree::list_worktrees(repo_root)? {
        if canonical_parent_path(&registered.path)? == expected {
            return Ok(Some(registered));
        }
    }
    Ok(None)
}

fn canonical_parent_path(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let mut parent = absolute.parent().context("Worktree path has no parent")?;
    let mut missing = vec![absolute.file_name().context("Worktree path has no name")?];
    loop {
        match parent.canonicalize() {
            Ok(mut resolved) => {
                for component in missing.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    parent
                        .file_name()
                        .context("Worktree ancestor has no name")?,
                );
                parent = parent.parent().context("Worktree ancestor has no parent")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// List entries whose ownership cannot be established after Git removed its admin dir.
fn unknown_entries(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("Failed to inspect {}", dir.display()))?
        {
            let entry = entry.with_context(|| format!("Failed to inspect {}", dir.display()))?;
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path)
                .with_context(|| format!("Failed to inspect {}", path.display()))?;
            let rel = path.strip_prefix(root)?.to_string_lossy();
            if !is_scaffold(&rel, &meta) {
                found.push(rel.into_owned());
                if found.len() > MAX_BLOCKING_PATHS {
                    return Ok(found);
                }
            }
            if meta.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(found)
}

/// Recognized scaffold directory names; names alone do not prove ownership.
fn is_scaffold(rel: &str, meta: &std::fs::Metadata) -> bool {
    meta.is_dir() && SCAFFOLD_DIRS.contains(&rel)
}

/// Entries under `root` (relative paths) selected by `pick`, found without
/// following symlinks and stopping once more than the listing cap is found.
fn walk(root: &Path, pick: &dyn Fn(&str, &std::fs::Metadata) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if pick(&rel, &meta) {
                found.push(rel);
                if found.len() > MAX_BLOCKING_PATHS {
                    return found;
                }
            }
            if meta.is_dir() {
                pending.push(path);
            }
        }
    }
    found
}

/// Paths blocking removal, labelled by source. Git status runs only when
/// `<worktree>/.git` exists and is pinned to the main repository's registered
/// administrative directory; otherwise leftover entries
/// come from a bounded filesystem walk.
fn blocking_paths(worktree_path: &Path, repo_root: &Path) -> String {
    if worktree_path.join(".git").exists() {
        if let Some(status) = git_status(worktree_path, repo_root) {
            return match cap_lines(&status.lines().map(str::to_string).collect::<Vec<_>>()) {
                s if s.is_empty() => s,
                s => format!("Blocking paths (git status --porcelain):\n{s}"),
            };
        }
    }
    let leftovers = walk(worktree_path, &|_, meta| !meta.is_dir());
    if leftovers.is_empty() {
        return String::new();
    }
    format!(
        "Leftover files on disk (filesystem listing, git no longer tracks this worktree):\n{}",
        cap_lines(&leftovers)
    )
}

fn git_status(worktree_path: &Path, repo_root: &Path) -> Option<String> {
    let output = WorktreeGit::pinned(repo_root, worktree_path)
        .ok()?
        .run(&["status", "--porcelain=v1", "--untracked-files=all"])
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string()
    })
}

/// Join `lines`, truncated to `MAX_BLOCKING_PATHS` with `… and N more`.
fn cap_lines(lines: &[String]) -> String {
    let mut shown = lines[..lines.len().min(MAX_BLOCKING_PATHS)].join("\n");
    if lines.len() > MAX_BLOCKING_PATHS {
        shown.push_str(&format!(
            "\n… and {} more",
            lines.len() - MAX_BLOCKING_PATHS
        ));
    }
    shown
}

#[cfg(test)]
#[path = "worktree_failure_tests.rs"]
mod tests;
