//! The ignored-path check reads the exclude rules of the checkout holding the
//! acceptance directory through `checkout_git`: a stage worktree's registered
//! repository, whatever its `.git` file names.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::references_ignored_path;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};

fn check_ignore_exit(dir: &Path, path: &str) -> Option<i32> {
    Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "check-ignore",
            "-q",
            "--",
            path,
        ])
        .current_dir(dir)
        .output()
        .unwrap()
        .status
        .code()
}

#[test]
fn an_honest_worktree_answers_from_its_own_ignore_rules() {
    let tmp = TempDir::new().unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
    std::fs::write(worktree.join(".gitignore"), "target/\n").unwrap();
    let nested = worktree.join("tests");

    assert!(references_ignored_path("./target/debug/loom", &worktree));
    assert!(references_ignored_path("cat ../target/out", &nested));
    assert!(!references_ignored_path("cat README.md", &worktree));
    assert!(!references_ignored_path("cat x_contract.rs", &nested));
    assert!(references_ignored_path("cat /usr/bin/env", &worktree));
}

/// The stage repoints its `.git` file at a git directory whose exclude file
/// ignores `secret.txt`. The positive control, run last, shows git that
/// follows the file says `secret.txt` is ignored.
#[test]
fn a_repointed_git_file_does_not_choose_the_answer() {
    let tmp = TempDir::new().unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
    std::fs::write(worktree.join("secret.txt"), "input\n").unwrap();
    plant_foreign_git_dir(&worktree);
    let exclude = tmp.path().join("foreign.git").join("info").join("exclude");
    std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
    std::fs::write(&exclude, "secret.txt\n").unwrap();

    assert!(!references_ignored_path("cat secret.txt", &worktree));
    assert!(!references_ignored_path(
        "cat ../secret.txt",
        &worktree.join("tests")
    ));

    assert_eq!(check_ignore_exit(&worktree, "secret.txt"), Some(0));
}
