//! A leftover `loom/<id>` branch met by `create_worktree` with a start point:
//! it is measured against that start point, recreated at it when it holds
//! nothing beyond it, and kept when the count cannot be measured.

use super::*;
use crate::fs::permissions::scratch_home::ScratchHome;
use serial_test::serial;
use std::process::Command;
use tempfile::TempDir;

const ID: &str = "s";
const BRANCH: &str = "loom/s";
/// A start point git cannot resolve.
const MISSING: &str = "no-such-rev";

/// Run `git` in `dir` with ambient configuration shut out, assert it
/// succeeded, and return its trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "git {args:?} failed: {stderr}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn tip(dir: &Path, rev: &str) -> String {
    git(dir, &["rev-parse", rev])
}

/// A repository whose `main` holds three commits; returns it with their ids,
/// oldest first.
fn repo_with_history() -> (TempDir, [String; 3]) {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    let tips = ["one", "two", "three"].map(|name| {
        std::fs::write(root.join(name), name).unwrap();
        git(root, &["add", name]);
        git(root, &["commit", "-q", "-m", name]);
        tip(root, "HEAD")
    });
    (repo, tips)
}

#[test]
#[serial]
fn a_leftover_branch_with_nothing_beyond_the_start_point_is_recreated_at_it() {
    let (repo, [leftover, accepted, live]) = repo_with_history();
    let root = repo.path();
    git(root, &["branch", BRANCH, &leftover]);
    assert_eq!(
        tip(root, "main"),
        live,
        "the live base is the newest commit"
    );
    let _home = ScratchHome::set();

    let worktree = create_worktree(ID, root, Some("main"), Some(&accepted)).unwrap();

    assert_eq!(
        tip(root, BRANCH),
        accepted,
        "the branch was not recreated at the start point"
    );
    assert_eq!(tip(&worktree.path, "HEAD"), accepted);
    assert_eq!(
        git(&worktree.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        BRANCH
    );
    assert_eq!(tip(root, "main"), live, "the live base moved");
}

#[test]
fn commits_are_counted_against_the_start_point_not_the_live_base() {
    let (repo, [_, accepted, live]) = repo_with_history();
    let root = repo.path();
    git(root, &["branch", BRANCH, &live]);

    let (base, ahead) = commits_beyond_base(BRANCH, root, Some("main"), Some(&accepted));

    assert_eq!(base, accepted);
    assert_eq!(ahead, 1, "the branch holds one commit past the start point");
    assert_eq!(
        commits_ahead_of(BRANCH, "main", root).unwrap(),
        0,
        "against the live base it would count nothing and lose its only ref"
    );
}

#[test]
fn a_start_point_that_does_not_resolve_counts_one_commit() {
    let (repo, [leftover, ..]) = repo_with_history();
    let root = repo.path();
    git(root, &["branch", BRANCH, &leftover]);

    let (base, ahead) = commits_beyond_base(BRANCH, root, Some("main"), Some(MISSING));

    assert_eq!(base, MISSING);
    assert_eq!(ahead, 1, "an unmeasurable count must not read as zero");
}

#[test]
fn a_start_point_that_does_not_resolve_keeps_the_leftover_branch() {
    let (repo, [leftover, ..]) = repo_with_history();
    let root = repo.path();
    git(root, &["branch", BRANCH, &leftover]);
    let worktree_path = root.join(".worktrees").join(ID);
    let worktree_path_str = worktree_path.to_string_lossy().to_string();
    let add_args = [
        "worktree",
        "add",
        "-b",
        BRANCH,
        worktree_path_str.as_str(),
        MISSING,
    ];

    recreate_or_reuse_branch(ID, root, &add_args, Some("main"), Some(MISSING)).unwrap();

    assert_eq!(
        tip(root, BRANCH),
        leftover,
        "the branch was deleted or moved"
    );
    assert_eq!(tip(&worktree_path, "HEAD"), leftover);
    assert_eq!(
        git(&worktree_path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        BRANCH
    );
}
