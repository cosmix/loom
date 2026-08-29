//! Working-directory resolution shared by every memory command handler.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::env;

use crate::fs::memory::init_memory_dir;
use crate::git::worktree::{find_repo_root_from_cwd, find_worktree_root_from_cwd};

/// Sentinel stage ID used by the four recording commands (`note`, `decision`,
/// `change`, `question`) when neither `--stage` nor `LOOM_STAGE_ID` supplies
/// one. Lets ad-hoc/interactive sessions (no active loom plan) still record
/// insights instead of erroring outright.
pub(super) const AD_HOC_STAGE_ID: &str = "ad-hoc";

/// Get the .work directory, handling worktree symlinks
///
/// When called from within a worktree (or its subdirectory), finds the worktree root
/// which has a `.work` symlink pointing to the main repo's `.work`.
/// When called from the main repo, walks up to find the repo root's `.work`.
fn get_work_dir() -> Result<std::path::PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;

    // First check if we're in a worktree
    if let Some(worktree_root) = find_worktree_root_from_cwd(&cwd) {
        let work_dir = worktree_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Not in a worktree (or worktree missing .work) - find repo root
    if let Some(repo_root) = find_repo_root_from_cwd(&cwd) {
        let work_dir = repo_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Fallback: check current directory (original behavior)
    let work_dir = cwd.join(".work");
    if work_dir.exists() {
        return Ok(work_dir);
    }

    bail!(".work directory not found. Run 'loom init' first.");
}

/// Get the .work directory for RECORDING commands, creating it if necessary.
///
/// Tries the existing `get_work_dir` search first, unchanged. If nothing is
/// found and cwd is inside a git repo, creates `<repo_root>/.work/memory/`
/// (via `init_memory_dir`) so `note`/`decision`/`change`/`question` still
/// work from an ad-hoc session with no active loom plan. `find_repo_root_from_cwd`
/// already resolves to the main repo root even when cwd is inside a
/// `.worktrees/<stage>` checkout, so this never creates a second `.work`
/// alongside a worktree's `.work` symlink. Read-only commands (`query`,
/// `list`, `show`) must NOT call this — they degrade instead of creating
/// anything.
///
/// Still fails with the original message when cwd is not inside a git repo.
pub(super) fn get_or_create_work_dir() -> Result<std::path::PathBuf> {
    if let Ok(work_dir) = get_work_dir() {
        return Ok(work_dir);
    }

    let cwd = env::current_dir().context("Failed to get current directory")?;
    // `find_repo_root_from_cwd` falls back to returning `cwd` itself when it
    // walks all the way to the filesystem root without finding a `.git`, so
    // `Some` alone doesn't mean "inside a git repo" - confirm the candidate
    // root actually has a `.git` entry before trusting it. A bare
    // `.exists()` isn't enough either: the walk-up has no ceiling, so from a
    // cwd with no repo in its own ancestry (e.g. a scratch directory under
    // the OS-shared temp root) it can climb all the way to an unrelated
    // ambient `.git` sitting far above any real project - even an empty
    // impostor directory that merely happens to be named `.git` - and treat
    // it as this call's repo root. `is_real_git_dir` requires the structure
    // a real repo always has, not just the name.
    let repo_root =
        find_repo_root_from_cwd(&cwd).filter(|root| is_real_git_dir(&root.join(".git")));
    let Some(repo_root) = repo_root else {
        bail!(".work directory not found. Run 'loom init' first.");
    };

    let work_dir = repo_root.join(".work");
    init_memory_dir(&work_dir)?;

    eprintln!(
        "{} No .work directory found; recording to {} (stage '{}')",
        "ℹ".blue(),
        work_dir.display(),
        AD_HOC_STAGE_ID
    );

    Ok(work_dir)
}

/// Get the .work directory for READ-ONLY commands (`query`, `list`, `show`).
///
/// Returns `None` instead of erroring when no `.work` exists, so these
/// commands degrade gracefully rather than failing. This matters because
/// `loom memory list` is the first step of the post-compaction recovery flow
/// (see CLAUDE.md Rule 3b) - a hard failure there would derail recovery
/// before it starts. Unlike `get_or_create_work_dir`, this never creates
/// anything.
pub(super) fn get_work_dir_readonly() -> Option<std::path::PathBuf> {
    get_work_dir().ok()
}

/// True when `git_path` (a candidate `<root>/.git`) looks like a real git
/// directory or worktree pointer, not merely a path that happens to be
/// named `.git`.
///
/// A worktree's `.git` is a FILE containing `gitdir: <path>` - git always
/// writes that content, so existence alone is a reliable signal. A real
/// `.git` DIRECTORY, whether from `git init`, `git clone`, or a bare repo,
/// always contains a `HEAD` file immediately; an ancestor directory that
/// merely happens to be named `.git` (for example one left behind, empty,
/// by an unrelated process sharing the same OS temp root) does not. Bare
/// existence would accept both; this rejects the impostor.
fn is_real_git_dir(git_path: &std::path::Path) -> bool {
    if git_path.is_file() {
        return true;
    }
    git_path.is_dir() && git_path.join("HEAD").exists()
}

/// Validate stage ID to prevent path traversal attacks
pub(super) fn validate_stage_id(id: &str) -> Result<()> {
    if id.contains('/') || id.contains("..") || id.contains('\\') {
        bail!("Invalid stage ID: contains path separators");
    }
    Ok(())
}
