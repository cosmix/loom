//! The content manifest holds exactly what is on disk: commits, staging and
//! index rewrites leave it alone, and every byte, mode bit and file changes
//! it.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serial_test::serial;
use sha2::{Digest, Sha256};

use super::hash_manifest;
use crate::git::target_guard::test_support::{git, repo};
use crate::git::worktree::WorktreeGit;

fn manifest(root: &Path) -> Option<String> {
    let mut hasher = Sha256::new();
    let mut remaining = u64::MAX;
    let git = WorktreeGit::discovered(root);
    hash_manifest(&git, root, &mut remaining, &mut hasher)?;
    Some(hex::encode(hasher.finalize()))
}

#[test]
#[serial]
fn the_same_content_staged_unstaged_or_committed_has_one_manifest() {
    let repo = repo();
    let root = &repo.root;
    std::fs::write(root.join("README.md"), "edited\n").unwrap();
    std::fs::write(root.join("new.txt"), "new\n").unwrap();
    let unstaged = manifest(root).unwrap();

    git(root, &["add", "README.md", "new.txt"]);
    let staged = manifest(root).unwrap();
    git(root, &["commit", "-q", "-m", "same content"]);
    let committed = manifest(root).unwrap();

    assert_eq!(staged, unstaged, "staging changed the manifest");
    assert_eq!(committed, unstaged, "a commit of the same bytes changed it");
}

#[test]
#[serial]
fn every_change_on_disk_changes_the_manifest() {
    let repo = repo();
    let root = &repo.root;
    std::fs::write(root.join("extra.txt"), "x\n").unwrap();
    let base = manifest(root).unwrap();
    let changed = |change: &dyn Fn()| {
        change();
        let after = manifest(root).unwrap();
        assert_ne!(after, base);
        after
    };

    let byte = changed(&|| std::fs::write(root.join("README.md"), "readmf\n").unwrap());
    std::fs::write(root.join("README.md"), "readme\n").unwrap();
    assert_eq!(
        manifest(root).unwrap(),
        base,
        "restoring the byte restores it"
    );
    let mode = changed(&|| set_mode(&root.join("README.md"), 0o755));
    set_mode(&root.join("README.md"), 0o644);
    let removed = changed(&|| std::fs::remove_file(root.join("extra.txt")).unwrap());
    let added = changed(&|| std::fs::write(root.join("other.txt"), "o\n").unwrap());

    assert_ne!(byte, mode);
    assert_ne!(removed, added);
}

#[test]
#[serial]
fn a_deleted_tracked_file_leaves_the_manifest() {
    let repo = repo();
    let root = &repo.root;
    std::fs::write(root.join("kept.txt"), "k\n").unwrap();
    let with_readme = manifest(root).unwrap();

    std::fs::remove_file(root.join("README.md")).unwrap();
    let deleted = manifest(root).unwrap();
    git(root, &["rm", "-q", "--cached", "README.md"]);

    assert_ne!(deleted, with_readme);
    assert_eq!(
        manifest(root).unwrap(),
        deleted,
        "unstaged or staged delete"
    );
}

#[test]
#[serial]
fn an_entry_git_does_not_compare_refuses_the_manifest() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let repo = repo();
        assert!(manifest(&repo.root).is_some());
        git(&repo.root, &["update-index", flag, "README.md"]);
        assert_eq!(manifest(&repo.root), None, "{flag}");
    }
}

#[test]
#[serial]
fn a_symlink_refuses_the_manifest() {
    let repo = repo();
    std::os::unix::fs::symlink("README.md", repo.root.join("link")).unwrap();
    assert_eq!(manifest(&repo.root), None);
}

#[test]
#[serial]
fn fingerprinting_never_rewrites_the_index() {
    let repo = repo();
    let index = repo.root.join(".git/index");
    // New stat data, same bytes: a locking `git status` would refresh the
    // index entry and write it back.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(repo.root.join("README.md"), "readme\n").unwrap();
    let before = std::fs::read(&index).unwrap();

    let first = manifest(&repo.root).unwrap();

    assert_eq!(std::fs::read(&index).unwrap(), before);
    assert_eq!(manifest(&repo.root).unwrap(), first);
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}
