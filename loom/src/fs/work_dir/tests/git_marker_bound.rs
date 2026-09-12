//! Regression tests for `nearest_git_root`'s structural `.git` validation:
//! an ancestor merely NAMED `.git` — no `HEAD`, no gitdir file — must not
//! bound, or satisfy, the upward workspace search.

use std::fs;

use tempfile::TempDir;

use super::super::{Layout, WorkDir};
use super::plant_workspace;

#[test]
fn an_empty_git_directory_does_not_bound_the_walk() {
    let temp = TempDir::new().unwrap();
    let ancestor = temp.path().join("ancestor");
    fs::create_dir_all(ancestor.join(".git")).unwrap();
    plant_workspace(&ancestor, Layout::Nested);

    let inner = ancestor.join("not-a-repo").join("deeper");
    fs::create_dir_all(&inner).unwrap();

    let wd = WorkDir::new(&inner).unwrap();
    assert_eq!(
        wd.root(),
        inner.join(".loom").join("work"),
        "an ancestor .git with no HEAD must not bound the walk or be adopted"
    );
}

/// Positive control for the test above: the same ancestor with `.git/HEAD`
/// present IS adopted, proving non-adoption there comes from `HEAD`'s
/// absence, not from the directory layout.
#[test]
fn the_same_layout_with_head_present_is_adopted() {
    let temp = TempDir::new().unwrap();
    let ancestor = temp.path().join("ancestor");
    fs::create_dir_all(ancestor.join(".git")).unwrap();
    fs::write(ancestor.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    let workspace = plant_workspace(&ancestor, Layout::Nested);

    let inner = ancestor.join("not-a-repo").join("deeper");
    fs::create_dir_all(&inner).unwrap();

    let wd = WorkDir::new(&inner).unwrap();
    assert_eq!(
        wd.root().canonicalize().unwrap(),
        workspace.canonicalize().unwrap()
    );
}

/// A `.git` FILE (a linked worktree's pointer) counts on existence alone, so
/// it still bounds — and satisfies — the walk with no `HEAD` involved.
#[test]
fn a_git_file_still_bounds_the_walk() {
    let temp = TempDir::new().unwrap();
    let ancestor = temp.path().join("ancestor");
    fs::create_dir_all(&ancestor).unwrap();
    fs::write(ancestor.join(".git"), "gitdir: /somewhere\n").unwrap();
    let workspace = plant_workspace(&ancestor, Layout::Nested);

    let inner = ancestor.join("not-a-repo").join("deeper");
    fs::create_dir_all(&inner).unwrap();

    let wd = WorkDir::new(&inner).unwrap();
    assert_eq!(
        wd.root().canonicalize().unwrap(),
        workspace.canonicalize().unwrap()
    );
}
