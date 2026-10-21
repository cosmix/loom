//! The pass-cache fingerprint reads a stage worktree through its registered
//! git directory, whatever the worktree's `.git` file names: the host child
//! computes it outside every sandbox.

use serial_test::serial;
use tempfile::TempDir;

use super::{capture, ExecutionIdentity};
use crate::git::worktree::WorktreeGit;
use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use crate::verify::criteria::confine::{prepare_confined, CommandSpec};

/// The positive control runs last: git that follows the repointed `.git`
/// file does run the filter, so its absence before comes from the pin.
#[test]
#[serial]
fn a_repointed_git_file_neither_runs_its_filter_nor_moves_the_fingerprint() {
    if skip_unless(
        confined_commands_runnable(),
        "a_repointed_git_file_neither_runs_its_filter_nor_moves_the_fingerprint",
        "loom's confinement cannot start here",
    ) {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
    let spec = CommandSpec::shell("printf ok");
    let confined = CommandConfinement::Confined;
    let prepared = prepare_confined(&spec, Some(&worktree), Some(&worktree), confined).unwrap();
    let identity = ExecutionIdentity::from_prepared(&prepared, confined).unwrap();
    let git_file = worktree.join(".git");
    let registered = std::fs::read(&git_file).unwrap();
    let marker = plant_foreign_git_dir(&worktree);
    let planted = std::fs::read(&git_file).unwrap();

    let attacked = capture(&worktree, &identity);

    assert!(
        !marker.exists(),
        "the fingerprint ran the stage's clean filter"
    );
    std::fs::write(&git_file, &registered).unwrap();
    let honest = capture(&worktree, &identity);
    assert!(honest.is_some(), "an honest worktree is fingerprinted");
    assert_eq!(attacked, honest);

    std::fs::write(&git_file, &planted).unwrap();
    WorktreeGit::discovered(&worktree)
        .run(&["status", "--porcelain"])
        .unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}
