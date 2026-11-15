//! Tests for the root and HEAD queries, against real temporary repositories.

use std::process::Command;

use tempfile::TempDir;

use super::*;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Revision Test"]);
    git(dir.path(), &["config", "user.email", "rev@example.invalid"]);
    dir
}

fn commit(dir: &Path) {
    std::fs::write(dir.join("a.txt"), "a\n").expect("write");
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "a"]);
}

#[test]
fn toplevel_from_a_subdirectory_is_the_work_tree_root() {
    let repo = init_repo();
    let sub = repo.path().join("sub dir");
    std::fs::create_dir(&sub).expect("mkdir");

    let root = toplevel(&sub).expect("inside a repository");

    assert_eq!(
        root.canonicalize().expect("root exists"),
        repo.path().canonicalize().expect("repo exists")
    );
}

#[test]
fn toplevel_outside_a_repository_names_the_directory() {
    let dir = TempDir::new().expect("tempdir");
    // A temp directory inside some checkout would resolve to that checkout.
    let ceiling = dir.path().parent().expect("tempdir has a parent");
    let probe = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir.path())
        .env("GIT_CEILING_DIRECTORIES", ceiling)
        .output()
        .expect("git runs");
    if probe.status.success() {
        return;
    }

    let error = toplevel(dir.path()).expect_err("not a repository");

    assert!(
        format!("{error:#}").contains(&dir.path().display().to_string()),
        "{error:#}"
    );
}

#[test]
fn unborn_branch_has_no_head_commit() {
    let repo = init_repo();
    let git = WorktreeGit::discovered(repo.path());

    assert_eq!(head_commit(&git).expect("unborn is not an error"), None);
    assert!(!has_commits(&git).expect("unborn is not an error"));
    let error = require_head_commit(&git).expect_err("no commit");
    assert_eq!(format!("{error:#}"), "HEAD has no commit");
}

#[test]
fn branch_naming_a_missing_object_is_an_error_not_unborn() {
    let repo = init_repo();
    let missing = "1".repeat(40);
    let git_dir = repo.path().join(".git");
    let head = std::fs::read_to_string(git_dir.join("HEAD")).expect("HEAD");
    let reference = head.trim().strip_prefix("ref: ").expect("symbolic HEAD");
    std::fs::write(git_dir.join(reference), format!("{missing}\n")).expect("write ref");

    let error = head_commit(&WorktreeGit::discovered(repo.path())).expect_err("damaged HEAD");

    let message = format!("{error:#}");
    assert!(message.contains(&missing), "{message}");
    assert!(message.contains("not a readable commit"), "{message}");
}

#[test]
fn committed_head_gives_its_id() {
    let repo = init_repo();
    commit(repo.path());
    let git = WorktreeGit::discovered(repo.path());

    let id = head_commit(&git).expect("head").expect("a commit");

    assert!(is_object_id(&id), "{id:?}");
    assert_eq!(require_head_commit(&git).expect("a commit"), id);
    assert!(has_commits(&git).expect("head"));
}

#[test]
fn head_outside_a_repository_is_an_error() {
    let dir = TempDir::new().expect("tempdir");
    let nested = dir.path().join("not-a-repo");
    std::fs::create_dir(&nested).expect("mkdir");
    // `.git` as a plain file that points nowhere makes discovery fail here.
    std::fs::write(nested.join(".git"), "gitdir: /nonexistent/loom-test\n").expect("write");

    let result = head_commit(&WorktreeGit::discovered(&nested));

    assert!(result.is_err(), "{result:?}");
}

#[test]
fn object_ids_are_sha1_or_sha256_hex() {
    assert!(is_object_id(&"a".repeat(40)));
    assert!(is_object_id(&"0".repeat(64)));
    assert!(!is_object_id(&"a".repeat(39)));
    assert!(!is_object_id(&"g".repeat(40)));
    assert!(!is_object_id(""));
}
