//! The pass cache fingerprints the branch `HEAD` names in a linked worktree,
//! beside the tree's content, so a script running git stays cacheable until
//! the branch changes; commits and index rewrites are no input.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serial_test::serial;
use tempfile::TempDir;

use crate::git::target_guard::test_support::{git, repo, Repo};
use crate::models::stage::{AcceptanceCriterion, CommandConfinement, Stage};
use crate::verify::criteria::cache_fingerprint::{capture, ExecutionIdentity};
use crate::verify::criteria::confine::{prepare_confined, CommandSpec};
use crate::verify::criteria::{run_acceptance_with_config, CachePolicy, CriteriaConfig};

/// A repository whose committed `branch.sh` runs git, and its linked worktree
/// `.worktrees/a`, where loom places a stage's tree.
fn linked_worktree() -> (Repo, PathBuf) {
    let repo = repo();
    std::fs::write(repo.root.join("branch.sh"), "git branch --show-current\n").unwrap();
    git(&repo.root, &["add", "branch.sh"]);
    git(&repo.root, &["commit", "-q", "-m", "branch script"]);
    git(
        &repo.root,
        &["worktree", "add", "-q", "-b", "a", ".worktrees/a"],
    );
    let tree = repo.root.join(".worktrees/a");
    (repo, tree)
}

fn identity(tree: &Path) -> ExecutionIdentity {
    let prepared = prepare_confined(
        &CommandSpec::shell("printf ok"),
        Some(tree),
        Some(tree),
        CommandConfinement::Confined,
    )
    .unwrap();
    ExecutionIdentity::from_prepared(&prepared, CommandConfinement::Confined).unwrap()
}

// Each change keeps the tree's content.
#[test]
#[serial]
fn only_the_branch_head_names_changes_a_linked_worktree_fingerprint() {
    let (_repo, tree) = linked_worktree();
    let identity = identity(&tree);
    let clean = capture(&tree, &identity).unwrap();
    assert_eq!(capture(&tree, &identity).unwrap(), clean);

    std::fs::write(tree.join("README.md"), "readme\n").unwrap();
    git(&tree, &["add", "README.md"]);
    git(&tree, &["update-index", "--really-refresh"]);
    assert_eq!(
        capture(&tree, &identity).unwrap(),
        clean,
        "an index rewrite"
    );

    git(&tree, &["commit", "-q", "--allow-empty", "-m", "moved"]);
    assert_eq!(capture(&tree, &identity).unwrap(), clean, "a new commit");

    git(&tree, &["checkout", "-q", "-b", "renamed"]);
    assert_ne!(capture(&tree, &identity).unwrap(), clean, "another branch");
}

#[test]
#[serial]
fn a_script_running_git_is_cached_until_head_changes() {
    if crate::process::sandbox_probe::skip_unless(
        crate::verify::criteria::host_launcher().is_some() && which::which("git").is_ok(),
        "a_script_running_git_is_cached_until_head_changes",
        "OS sandbox launcher or git absent",
    ) {
        return;
    }
    let (_repo, tree) = linked_worktree();
    let cache = TempDir::new().unwrap();
    let config = CriteriaConfig::with_timeout(Duration::from_secs(30))
        .with_cache_dir(cache.path())
        .with_cache_policy(CachePolicy::Use)
        .with_tree_root(Some(&tree));
    let mut stage = Stage::new("git-state-cache-test".to_string(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple("sh branch.sh".to_string()));
    let run = || run_acceptance_with_config(&stage, Some(&tree), &config).unwrap();

    let first = run();
    let hit = run();
    git(&tree, &["checkout", "-q", "-b", "renamed"]);
    let moved = run();

    assert!(first.all_passed(), "{:?}", first.results());
    assert_eq!(first.results()[0].stdout.trim(), "a");
    assert!(!first.results()[0].cached && hit.results()[0].cached);
    assert!(moved.all_passed() && !moved.results()[0].cached);
    assert_eq!(moved.results()[0].stdout.trim(), "renamed");
}
