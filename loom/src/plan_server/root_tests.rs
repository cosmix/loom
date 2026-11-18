//! Tests for project root resolution.

use super::root::resolve_project;
use crate::test_git::{git, TestRepo};

#[test]
fn outside_repository_names_directory() {
    let dir = tempfile::tempdir().unwrap();
    let err = resolve_project(dir.path()).unwrap_err().to_string();
    assert!(err.contains("loom plan needs a git repository"));
    assert!(err.contains(&dir.path().display().to_string()));
}

#[test]
fn subdirectory_resolves_canonical_top_level() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project with space ");
    std::fs::create_dir(&root).unwrap();
    TestRepo::builder()
        .identity("Test", "test@example.com")
        .no_seed()
        .init_in(&root);
    git(&root, &["commit", "--allow-empty", "-m", "initial"]);
    let subdir = root.join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    assert_eq!(
        resolve_project(&subdir).unwrap(),
        root.canonicalize().unwrap()
    );
}

#[test]
fn stage_worktree_names_main_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("main");
    let worktree = dir.path().join("stage");
    std::fs::create_dir(&root).unwrap();
    TestRepo::builder()
        .identity("Test", "test@example.com")
        .no_seed()
        .init_in(&root);
    git(&root, &["commit", "--allow-empty", "-m", "initial"]);
    git(
        &root,
        &[
            "worktree",
            "add",
            "-b",
            "loom/demo",
            worktree.to_str().unwrap(),
        ],
    );
    let err = resolve_project(&worktree).unwrap_err().to_string();
    assert!(err.contains("loom stage worktree (branch loom/demo)"));
    assert!(err.contains(&root.canonicalize().unwrap().display().to_string()));
    assert!(err.contains(&worktree.canonicalize().unwrap().display().to_string()));
}

#[test]
fn empty_repository_resolves() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    assert_eq!(
        resolve_project(dir.path()).unwrap(),
        dir.path().canonicalize().unwrap()
    );
}
