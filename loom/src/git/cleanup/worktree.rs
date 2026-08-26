//! Worktree cleanup operations

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;

use crate::git::runner::run_git_checked;

/// Cap on how many blocking paths `refusal_error` lists verbatim. The
/// message is persisted into stage frontmatter and shipped in every status
/// frame (2 MiB IPC cap), so it must be bounded.
const MAX_BLOCKING_PATHS: usize = 20;

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
                Err(refusal_error(&worktree_path, e))
            }
        }
    }
}

/// Paths `git worktree remove` would refuse over, one per line, for the
/// non-forced failure message. Capped at `MAX_BLOCKING_PATHS` lines with a
/// trailing `… and N more` summary — this text is persisted into stage
/// frontmatter and shipped in every status frame (2 MiB IPC cap), so it must
/// stay bounded. Empty when status cannot be read.
fn blocking_paths(worktree_path: &Path) -> String {
    let raw = run_git_checked(
        &["status", "--porcelain=v1", "--untracked-files=all"],
        worktree_path,
    )
    .map(|out| out.trim_end().to_string())
    .unwrap_or_default();
    cap_blocking_paths(&raw)
}

/// Truncate `raw` (one status line per line) to `MAX_BLOCKING_PATHS` lines,
/// appending `… and N more` when lines were dropped.
fn cap_blocking_paths(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = raw.lines().collect();
    if lines.len() <= MAX_BLOCKING_PATHS {
        return raw.to_string();
    }
    let mut result = lines[..MAX_BLOCKING_PATHS].join("\n");
    result.push_str(&format!(
        "\n… and {} more",
        lines.len() - MAX_BLOCKING_PATHS
    ));
    result
}

/// Build the non-forced-removal refusal error, naming which paths block
/// removal so the caller doesn't have to go spelunking with `git status`
/// themselves.
///
/// The headline stays deliberately generic: `git worktree remove` also
/// refuses on locked worktrees, submodules, and other conditions unrelated
/// to uncommitted files, so `e`'s own context carries the actual reason.
fn refusal_error(worktree_path: &Path, e: anyhow::Error) -> anyhow::Error {
    let mut message = format!(
        "git worktree remove refused for {} and force was not set; \
         leaving the worktree in place (git's reason is the underlying error).",
        worktree_path.display()
    );
    let blocking = blocking_paths(worktree_path);
    if !blocking.is_empty() {
        message.push_str(&format!(
            "\nBlocking paths (git status --porcelain):\n{blocking}"
        ));
    }
    e.context(message)
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

/// Repo-relative `.claude`/`CLAUDE.md` paths git tracks in `worktree_path`'s
/// repository (`git ls-files -- .claude CLAUDE.md`). Creation only plants
/// scaffold when the checkout carries none of its own (`setup_claude_directory`,
/// `setup_root_claude_md`), so anything git tracks there was never loom's to
/// remove. Empty when the query fails — unit tests call `remove_worktree_scaffold`
/// on plain temp dirs with no git repository at all.
fn tracked_scaffold_paths(worktree_path: &Path) -> HashSet<String> {
    run_git_checked(
        &["ls-files", "-z", "--", ".claude", "CLAUDE.md"],
        worktree_path,
    )
    .map(|out| {
        out.split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Remove only Loom-generated scaffold before non-forced Git removal.
///
/// Loom only removes the scaffold it planted. `.work` is always the symlink
/// `ensure_work_symlink` creates, so a non-symlink there is refused as wrong
/// (`remove_required_symlink`). The `.claude/` and root `CLAUDE.md` halves
/// both mirror creation's "plant only when absent" condition: `.claude/CLAUDE.md`
/// and root `CLAUDE.md` are scaffold only when they are the symlinks
/// `setup_claude_directory` / `setup_root_claude_md` create for repos that
/// carry none of their own. When the repo tracks either path, the worktree
/// checks it out as the repo's own file — not scaffold — and it is left in
/// place for `git worktree remove` (or `git status`) to judge, regardless of
/// whether it happens to be a symlink.
pub(crate) fn remove_worktree_scaffold(worktree_path: &Path) -> Result<()> {
    let tracked = tracked_scaffold_paths(worktree_path);
    remove_required_symlink(&worktree_path.join(".work"))?;
    let claude_dir = worktree_path.join(".claude");
    if claude_dir.is_symlink() {
        remove_if_symlink(&claude_dir)?;
    } else if claude_dir.exists() {
        remove_known_claude_scaffold(&claude_dir, &tracked)?;
    }
    if !tracked.contains("CLAUDE.md") {
        remove_if_symlink(&worktree_path.join("CLAUDE.md"))?;
    }
    Ok(())
}

/// Remove generated `.claude/` scaffold entries, skipping anything Loom did
/// not create.
///
/// Only `CLAUDE.md` (the generated symlink) and `settings.json` /
/// `settings.local.json` (loom-generated files) are ever candidates for
/// removal, and only when `tracked` (see `tracked_scaffold_paths`) does not
/// already track them under the repo — a tracked entry is the repo's own,
/// not scaffold. Any other entry — a `.cc-writes/` runtime directory Claude
/// Code creates, user notes — is left untouched. The directory itself is
/// removed only once it is empty; a non-empty `.claude/` is left for `git
/// worktree remove` (or `git status`) to judge.
fn remove_known_claude_scaffold(claude_dir: &Path, tracked: &HashSet<String>) -> Result<()> {
    for entry in std::fs::read_dir(claude_dir)
        .with_context(|| format!("Failed to inspect {}", claude_dir.display()))?
    {
        let entry = entry.context("Failed to inspect .claude scaffold entry")?;
        let path = entry.path();
        if let Some(name) = entry.file_name().to_str() {
            remove_claude_entry(&path, name, tracked)?;
        }
    }

    let is_empty = std::fs::read_dir(claude_dir)
        .with_context(|| format!("Failed to inspect {}", claude_dir.display()))?
        .next()
        .is_none();
    if is_empty {
        std::fs::remove_dir(claude_dir).with_context(|| {
            format!(
                "Failed to remove scaffold directory {}",
                claude_dir.display()
            )
        })?;
    }
    Ok(())
}

/// Remove a single `.claude/` entry if it is generated scaffold; otherwise
/// leave it in place untouched.
fn remove_claude_entry(path: &Path, name: &str, tracked: &HashSet<String>) -> Result<()> {
    if tracked.contains(&format!(".claude/{name}")) {
        return Ok(());
    }
    match name {
        "CLAUDE.md" => remove_if_symlink(path),
        "settings.json" | "settings.local.json" => {
            if path.is_dir() && !path.is_symlink() {
                // Unexpected directory where a generated file belongs; leave
                // it for git to judge rather than destroying it.
                return Ok(());
            }
            std::fs::remove_file(path)
                .with_context(|| format!("Failed to remove generated scaffold {}", path.display()))
        }
        _ => Ok(()),
    }
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
