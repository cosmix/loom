//! `extensions.worktreeConfig` read the way git reads it, as a boolean.

use super::super::test_support::{git_ok, init_repo};
use super::*;

#[test]
fn every_spelling_of_true_turns_on_the_worktree_hooks_path() {
    for spelling in ["true", "TRUE", "yes", "on", "1"] {
        let repo = init_repo();
        let root = repo.path();
        git_ok(root, &["config", "core.hooksPath", ".hooks-local"]);
        git_ok(root, &["config", "extensions.worktreeConfig", spelling]);
        git_ok(
            root,
            &["config", "--worktree", "core.hooksPath", ".hooks-worktree"],
        );

        assert_eq!(
            hooks_dir_prefix_checked(root).unwrap().as_deref(),
            Some(".hooks-worktree/"),
            "{spelling}"
        );
    }
}

/// Git expands `~/` before it looks for a hook, so this directory is in the
/// home directory, outside the repository, and no `~/...` path in the checkout
/// is the hooks directory.
#[test]
fn a_home_relative_hooks_path_is_not_a_directory_in_the_repository() {
    let repo = init_repo();
    let root = repo.path();
    git_ok(
        root,
        &["config", "core.hooksPath", "~/loom-hooks-elsewhere"],
    );

    assert_eq!(hooks_dir_prefix_checked(root).unwrap(), None);
}

#[test]
fn every_spelling_of_false_leaves_the_worktree_hooks_path_unread() {
    for spelling in ["false", "no", "off", "0"] {
        let repo = init_repo();
        let root = repo.path();
        git_ok(root, &["config", "core.hooksPath", ".hooks-local"]);
        git_ok(root, &["config", "extensions.worktreeConfig", spelling]);

        assert_eq!(
            hooks_dir_prefix_checked(root).unwrap().as_deref(),
            Some(".hooks-local/"),
            "{spelling}"
        );
    }
}
