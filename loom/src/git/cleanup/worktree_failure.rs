//! Failure handling for non-forced `git worktree remove`: the operator-facing
//! error text and recovery of a worktree git already half-removed.

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::Path;

use crate::git::runner::{run_git_checked, run_git_with_env};
use crate::verify::tool_artifacts::NAMES;

/// Cap on how many paths an error lists verbatim. The message is persisted
/// into stage frontmatter and shipped in every status frame (2 MiB IPC cap),
/// so it must be bounded.
const MAX_BLOCKING_PATHS: usize = 20;

/// Symlinks loom plants in a worktree (worktree-relative).
const SCAFFOLD_SYMLINKS: [&str; 4] = [".loom/work", ".work", "CLAUDE.md", ".claude/CLAUDE.md"];
/// Generated regular files under `.claude/`.
const SCAFFOLD_CLAUDE_FILES: [&str; 2] = [".claude/settings.json", ".claude/settings.local.json"];

/// Build the failure error for a non-forced removal. Only git's dirty-tree
/// refusal is described as a refusal; any other failure carries git's own
/// stderr (the underlying error) as the reason.
pub(super) fn removal_failure(worktree_path: &Path, e: anyhow::Error) -> anyhow::Error {
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
    let blocking = blocking_paths(worktree_path);
    if !blocking.is_empty() {
        message.push_str(&format!("\n{blocking}"));
    }
    e.context(message)
}

/// Whether git reports the worktree already dropped from its registry while
/// the directory is still on disk.
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
/// Returns `Ok(true)` once the leftover tree held only scaffold, was removed
/// and the registry pruned; `Ok(false)` when the failure is not that state.
/// Any other regular file left makes this fail naming those files and deletes
/// nothing. The caller has already vetted `worktree_path` with
/// `worktree_directory_exists`; removal never follows a symlink.
pub(super) fn recover_half_removed(
    worktree_path: &Path,
    repo_root: &Path,
    e: &anyhow::Error,
) -> Result<bool> {
    if !half_removed(worktree_path, e) {
        return Ok(false);
    }
    let unknown = walk(worktree_path, &|rel, meta| !is_scaffold(rel, meta));
    if !unknown.is_empty() {
        anyhow::bail!(
            "git removed worktree {} but files that are not loom scaffold remain; \
             nothing was deleted. Remaining files:\n{}",
            worktree_path.display(),
            cap_lines(&unknown)
        );
    }
    std::fs::remove_dir_all(worktree_path).with_context(|| {
        format!(
            "Failed to remove leftover scaffold in {}",
            worktree_path.display()
        )
    })?;
    run_git_checked(&["worktree", "prune"], repo_root)?;
    Ok(true)
}

/// Whether a non-directory entry is scaffold loom or the session sandbox
/// creates; directories are always acceptable (only files make them matter).
fn is_scaffold(rel: &str, meta: &std::fs::Metadata) -> bool {
    if meta.is_dir() {
        return true;
    }
    if meta.file_type().is_symlink() {
        return SCAFFOLD_SYMLINKS.contains(&rel);
    }
    meta.is_file()
        && (SCAFFOLD_CLAUDE_FILES.contains(&rel) || (NAMES.contains(&rel) && meta.len() == 0))
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
/// `<worktree>/.git` exists and is pinned below the worktree's parent so it
/// can never describe an enclosing repository; otherwise leftover entries
/// come from a bounded filesystem walk.
fn blocking_paths(worktree_path: &Path) -> String {
    if worktree_path.join(".git").exists() {
        if let Some(status) = git_status(worktree_path) {
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

fn git_status(worktree_path: &Path) -> Option<String> {
    let ceiling = worktree_path.parent()?;
    let output = run_git_with_env(
        &["status", "--porcelain=v1", "--untracked-files=all"],
        &[("GIT_CEILING_DIRECTORIES", OsStr::new(ceiling))],
        worktree_path,
    )
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
