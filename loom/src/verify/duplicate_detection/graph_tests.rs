//! The unwired and duplicate-symbol checks run on the host child, outside the
//! stage's sandbox: they read a stage worktree through its registered git
//! directory, whatever the worktree's `.git` file names.

use super::{changed_files, repository_root};
use crate::git::runner::run_git_checked;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use tempfile::TempDir;

/// The positive control runs last: the old call, git that follows the
/// repointed `.git` file, does run the filter, so its absence before comes
/// from the pin.
#[test]
fn a_repointed_git_file_neither_runs_its_filter_nor_changes_the_answers() {
    let tmp = TempDir::new().unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
    let git_file = worktree.join(".git");
    let registered = std::fs::read(&git_file).unwrap();
    let marker = plant_foreign_git_dir(&worktree);
    let planted = std::fs::read(&git_file).unwrap();

    let root = repository_root(&worktree).unwrap();
    let changed = changed_files(&root, "main", false).unwrap();
    let added = changed_files(&root, "main", true).unwrap();

    assert!(!marker.exists(), "the check ran the stage's clean filter");
    std::fs::write(&git_file, &registered).unwrap();
    assert_eq!(root, worktree.canonicalize().unwrap());
    assert_eq!(repository_root(&worktree).unwrap(), root);
    assert_eq!(changed_files(&root, "main", false).unwrap(), changed);
    assert_eq!(changed_files(&root, "main", true).unwrap(), added);
    assert!(changed.contains("README.md"), "{changed:?}");
    assert!(changed.contains("tests/x_contract.rs"), "{changed:?}");
    assert!(!added.contains("README.md"), "{added:?}");
    assert!(added.contains("tests/x_contract.rs"), "{added:?}");

    std::fs::write(&git_file, &planted).unwrap();
    let old_diff = [
        "diff",
        "--name-only",
        "-z",
        "--diff-filter=ACMRT",
        "HEAD",
        "--",
    ];
    run_git_checked(&old_diff, &worktree).unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}

/// Rename detection is off, so a moved file is listed as added at its new
/// path; with its old path gone it has no copy to match, so the move is no
/// duplicate.
#[cfg(feature = "source-graph")]
#[test]
fn a_moved_file_is_added_but_not_a_duplicate() {
    use crate::test_git::{git, TestRepo};

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    TestRepo::builder().no_seed().init_in(root);
    std::fs::write(
        root.join("a.rs"),
        "pub fn normalize_settings() -> u32 { 42 }\n",
    )
    .unwrap();
    git(root, &["add", "a.rs"]);
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "seed"],
    );
    git(root, &["switch", "-q", "-c", "feature"]);
    git(root, &["mv", "a.rs", "b.rs"]);
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "move"],
    );

    let added = changed_files(root, "main", true).unwrap();
    let duplicates = crate::verify::duplicate_detection::detect_duplicate_symbols(root, "main");

    assert_eq!(added.into_iter().collect::<Vec<_>>(), ["b.rs"]);
    assert!(duplicates.unwrap().is_empty());
}
