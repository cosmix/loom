//! Contract tests for the shared branch, status, root and HEAD queries.
//!
//! Each fixture is a TempDir repository built with the shared `git` helpers, isolated
//! from the caller's environment and global configuration, so the queries
//! under test see exactly the state each scenario describes.

use std::process::Command;

use tempfile::TempDir;

use crate::git::status_entries::{parse_porcelain_v1_z, StatusEntry};
use crate::git::worktree::WorktreeGit;
use crate::test_git::{git, TestRepo};

/// A repository with a local identity and no commit.
fn init_repo() -> TestRepo {
    TestRepo::builder().no_seed().create()
}

/// A repository with one commit on `main`.
fn repo_with_commit() -> TestRepo {
    TestRepo::builder()
        .seed("seed.txt", "seed\n", "seed")
        .create()
}

#[test]
fn tag_named_like_branch_keeps_stage_id() {
    let repo = repo_with_commit();
    git(repo.path(), &["checkout", "-q", "-b", "loom/s1"]);
    git(repo.path(), &["tag", "loom/s1"]);

    let stage_id = crate::commands::common::detect_stage_id_in(repo.path());

    assert_eq!(stage_id.as_deref(), Some("s1"));
}

#[test]
fn detached_head_has_no_branch() {
    let repo = repo_with_commit();
    git(repo.path(), &["checkout", "-q", "--detach"]);

    let branch = crate::git::branch::checked_out_branch_with(&WorktreeGit::discovered(repo.path()))
        .expect("a detached HEAD is not an error");

    assert_eq!(branch, None);
}

#[test]
fn porcelain_rename_origin_and_leading_space() {
    let entries = parse_porcelain_v1_z(b"R  new.rs\0old.rs\0 M a b.rs\0?? c.rs\0")
        .expect("well-formed listing parses");

    let expected = vec![
        StatusEntry {
            code: *b"R ",
            path: "new.rs".to_string(),
            origin: Some("old.rs".to_string()),
        },
        StatusEntry {
            code: *b" M",
            path: "a b.rs".to_string(),
            origin: None,
        },
        StatusEntry {
            code: *b"??",
            path: "c.rs".to_string(),
            origin: None,
        },
    ];
    assert_eq!(entries, expected);
}

#[test]
fn porcelain_rejects_non_utf8_path() {
    let parsed = parse_porcelain_v1_z(b"?? \xffname.rs\0");

    assert!(
        parsed.is_err(),
        "non-UTF-8 path must be refused, got {parsed:?}"
    );
}

#[test]
fn porcelain_rejects_unterminated_listing() {
    let parsed = parse_porcelain_v1_z(b" M a.rs");

    assert!(
        parsed.is_err(),
        "a listing not ending in NUL must be refused, got {parsed:?}"
    );
}

#[test]
fn working_tree_changes_unquoted() {
    let repo = repo_with_commit();
    std::fs::write(repo.path().join("quote\"d.txt"), "x\n").expect("write file");

    let changes =
        crate::git::branch::list_working_tree_changes(repo.path()).expect("status listing");

    assert_eq!(changes, vec!["quote\"d.txt".to_string()]);
}

#[test]
fn toplevel_refuses_non_repository() {
    let dir = TempDir::new().expect("tempdir");
    let probe = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir.path())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git runs");
    assert!(
        !probe.status.success(),
        "precondition: {} must not be inside a git repository",
        dir.path().display()
    );

    let root = crate::git::revision::toplevel(dir.path());

    assert!(
        root.is_err(),
        "toplevel outside a repository must be Err, got {root:?}"
    );
}

#[test]
fn head_commit_none_on_unborn() {
    let repo = init_repo();

    let head = crate::git::revision::head_commit(&WorktreeGit::discovered(repo.path()))
        .expect("an unborn branch is not an error");

    assert_eq!(head, None);
}
