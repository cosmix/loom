//! Shared helpers for the merge tests: scratch repositories on the shared git
//! fixtures, branch builders, and a snapshot of a checkout's visible state.

use crate::test_git::{git, git_unchecked, TestRepo};
// The merge tests keep their historical helper names; each is the shared fixture.
pub(super) use crate::test_git::{
    commit_file, git as git_ok, git as git_out, git_unchecked as isolated_git,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Repository on `main` with `a.txt` ("seed") committed.
pub(super) fn init_repo() -> TempDir {
    TestRepo::builder()
        .identity("t", "t@t.com")
        .seed("a.txt", "seed", "seed")
        .create()
        .into_temp_dir()
}

/// A work directory outside the repository, so the merge lock file does not
/// show up as an untracked file in it.
pub(super) fn lock_dir() -> TempDir {
    TempDir::new().unwrap()
}

/// Create `loom/<id>` from `main` with one commit per `(file, text)` pair,
/// then return to `main`.
pub(super) fn stage_branch(root: &Path, id: &str, files: &[(&str, &str)]) {
    git(root, &["checkout", "-b", &format!("loom/{id}"), "main"]);
    for (name, text) in files {
        commit_file(root, name, text, &format!("stage {name}"));
    }
    git(root, &["checkout", "main"]);
}

pub(super) fn rev(root: &Path, name: &str) -> String {
    git(root, &["rev-parse", name])
}

/// Everything an operator can see in a checkout: porcelain status plus the
/// bytes of every tracked or untracked file.
pub(super) fn snapshot(root: &Path) -> (String, Vec<(String, Vec<u8>)>) {
    let status = git(root, &["status", "--porcelain=v2", "--untracked-files=all"]);
    let listing = git_unchecked(root, &["ls-files", "-z", "--cached", "--others"]);
    let files = String::from_utf8_lossy(&listing.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(|p| {
            (
                p.to_string(),
                std::fs::read(root.join(p)).unwrap_or_default(),
            )
        })
        .collect();
    (status, files)
}

/// A sibling directory for a second worktree.
pub(super) fn sibling_dir() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("second");
    (temp, path)
}

/// Number of commit objects in the repository, reachable or not.
pub(super) fn commit_count(root: &Path) -> usize {
    git(
        root,
        &[
            "cat-file",
            "--batch-all-objects",
            "--batch-check=%(objecttype)",
        ],
    )
    .lines()
    .filter(|kind| *kind == "commit")
    .count()
}
