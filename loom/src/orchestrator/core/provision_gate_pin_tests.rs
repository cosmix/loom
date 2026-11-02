//! The listing around a provision runs on the host, in a worktree whose
//! `.git` file the stage controls: it is pinned. Its entries keep their columns.

use super::*;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};

/// A stage repointed its worktree's `.git` at a repository of its own whose
/// config names a clean filter: a host process that follows the pointer runs
/// it. The positive control runs last: git that follows the pointer does run
/// the filter, so its absence before comes from the pin.
#[test]
fn the_listing_around_a_provision_does_not_follow_a_forged_git_file() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().canonicalize().unwrap().join("repo");
    let worktree = contract_worktree(&repo, "alpha");
    let marker = plant_foreign_git_dir(&worktree);
    let work = prepare_work_dir(&repo);

    provision_worktree(&work, &[entry("true")], &worktree).unwrap();

    assert!(
        !marker.exists(),
        "the provision listing ran the stage's filter"
    );
    crate::git::worktree::WorktreeGit::pinned_in_project_of(&work, &worktree)
        .unwrap()
        .run_checked(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .unwrap();
    assert!(
        status_entries(&work, &worktree).is_some(),
        "the listing ran"
    );
    assert!(
        !marker.exists(),
        "the provision listing ran the stage's filter"
    );
    crate::git::worktree::WorktreeGit::discovered(&worktree)
        .run(&["status", "--porcelain"])
        .unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}

/// A worktree that cannot be pinned is the unreadable-listing path: no
/// comparison, and no git in it either.
#[test]
fn a_directory_no_repository_registers_skips_the_comparison() {
    let worktree = TempDir::new().unwrap();

    let (passed, stage) = gate(&["touch provisioned"], Vec::new(), worktree.path());

    assert!(passed);
    assert_eq!(stage.close_reason, None);
    assert!(worktree.path().join("provisioned").exists());
}

#[test]
fn status_entries_keep_their_leading_space_and_rename_origins() {
    let listing = " M web/bun.lock\0R  new.txt\0old.txt\0?? added\0";

    assert_eq!(
        split_status_entries(listing),
        [" M web/bun.lock", "R  new.txt\0old.txt", "?? added"]
    );
}

#[test]
fn a_rename_in_the_worktree_column_keeps_its_origin_in_one_entry() {
    assert_eq!(
        split_status_entries(" R new.txt\0old.txt\0"),
        [" R new.txt\0old.txt"]
    );
}

#[test]
fn a_renamed_file_is_reported_by_its_new_path() {
    assert_eq!(
        unignored_reason(&["R  new.txt\0old.txt"]),
        unignored_reason(&["R  new.txt"])
    );
}
