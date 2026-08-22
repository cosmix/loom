//! Ensure the working directory is a git repository.
//!
//! Loom assumes it runs inside a git work tree (branches, worktrees and the
//! `.work/` sidecar all lean on it). When it is first invoked in a plain
//! directory, `git init` is run on the spot so the rest of the pipeline sees
//! a well-formed repository instead of failing on its first git operation.
//!
//! Everything here is best-effort and quiet, matching the "never fatal"
//! discipline of the hooks: a missing `git` binary or a permission error is
//! swallowed rather than surfaced.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

/// Create a git repository at `path` if `path` is not already inside one.
///
/// This is the integration point wired into the command entry once the
/// working directory is known. It is deliberately infallible so a missing
/// repository can never abort a hook or command.
pub fn ensure_git_repo(path: &Path) {
    if inside_git_work_tree(path) {
        return;
    }
    let _ = Command::new("git").arg("init").current_dir(path).status();
}

/// Returns `true` when `path` (or one of its ancestors) is a git work tree.
fn inside_git_work_tree(path: &Path) -> bool {
    // Fast path: look for a `.git` entry at or above `path`. This covers the
    // common case without spawning git and works even when git is absent.
    // `.git` may be a directory (normal clone) or a file (worktree/submodule),
    // so `exists` is the right test.
    for current in path.ancestors() {
        if current.join(".git").exists() {
            return true;
        }
    }

    // Authoritative fallback: ask git itself. If git is absent the command
    // fails to spawn and we conservatively report "not a work tree".
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detects_an_existing_repo() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(inside_git_work_tree(dir.path()));
    }

    #[test]
    fn ensure_is_best_effort_and_never_panics() {
        let dir = tempdir().unwrap();
        ensure_git_repo(dir.path());
    }
}
