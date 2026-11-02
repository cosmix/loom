//! Cleanup acts on the repository it was given when the process inherits a git
//! redirection. `GIT_DIR` is process-wide, so each case runs in its own process.

use super::*;
use std::process::Command;

const REPO: &str = "LOOM_BATCH_TEST_REPO";

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t.com"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

fn repository() -> tempfile::TempDir {
    let repo = tempfile::TempDir::new().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    git(
        repo.path(),
        &["commit", "-q", "--allow-empty", "-m", "seed"],
    );
    repo
}

/// Run `child` (a test in this module) in a new process with `GIT_DIR` naming
/// `other` and [`REPO`] naming `repo`.
fn in_redirected_process(child: &str, repo: &Path, other: &Path) {
    let test = format!("{}::{child}", module_path!().split_once("::").unwrap().1);
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test, "--include-ignored", "--nocapture"])
        .env(REPO, repo)
        .env("GIT_DIR", other.join(".git"))
        .status()
        .unwrap();
    assert!(status.success(), "{child} failed");
}

#[test]
#[ignore = "run by needs_cleanup_ignores_an_inherited_git_dir in its own process"]
fn needs_cleanup_child() {
    let repo = std::env::var_os(REPO).unwrap();
    assert!(!needs_cleanup("redirected", Path::new(&repo)));
}

#[test]
fn needs_cleanup_ignores_an_inherited_git_dir() {
    let (repo, other) = (repository(), repository());
    git(other.path(), &["branch", "loom/redirected"]);
    in_redirected_process("needs_cleanup_child", repo.path(), other.path());
}

#[test]
#[ignore = "run by prune_worktrees_ignores_an_inherited_git_dir in its own process"]
fn prune_worktrees_child() {
    let repo = std::env::var_os(REPO).unwrap();
    prune_worktrees(Path::new(&repo)).unwrap();
}

#[test]
fn prune_worktrees_ignores_an_inherited_git_dir() {
    let (repo, other) = (repository(), repository());
    let linked = tempfile::TempDir::new().unwrap();
    let path = linked.path().join("gone");
    git(
        repo.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "gone",
            path.to_str().unwrap(),
        ],
    );
    std::fs::remove_dir_all(&path).unwrap();
    let admin = repo.path().join(".git/worktrees/gone");
    assert!(admin.exists());

    in_redirected_process("prune_worktrees_child", repo.path(), other.path());

    assert!(
        !admin.exists(),
        "the given repository's stale entry is pruned"
    );
}
