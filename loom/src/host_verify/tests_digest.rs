//! Tree digest against real repositories.

use std::path::Path;
use std::process::Command;

use sha2::Digest as _;
use tempfile::TempDir;

use super::digest::{knowledge_tree_digest, tree_digest};

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "-c", "user.name=t"])
        .args(["-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

fn write(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A repository with a tracked `a.txt`, a tracked knowledge file and an
/// ignore rule for `*.log`.
fn repository() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    write(dir, "a.txt", "one\n");
    write(dir, ".gitignore", "*.log\n");
    write(dir, "doc/loom/knowledge/note.md", "note\n");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
    tmp
}

#[test]
fn a_clean_committed_tree_hashes_the_empty_input() {
    let repo = repository();
    let empty = hex::encode(sha2::Sha256::digest(b""));
    assert_eq!(tree_digest(repo.path()).unwrap(), empty);
    assert_eq!(tree_digest(repo.path()).unwrap().len(), 64);
}

#[test]
fn a_tracked_edit_changes_the_digest() {
    let repo = repository();
    let clean = tree_digest(repo.path()).unwrap();
    write(repo.path(), "a.txt", "two\n");
    let edited = tree_digest(repo.path()).unwrap();
    assert_ne!(clean, edited);
    assert_eq!(edited, tree_digest(repo.path()).unwrap());
}

#[test]
fn an_untracked_file_changes_the_digest() {
    let repo = repository();
    let clean = tree_digest(repo.path()).unwrap();
    write(repo.path(), "new.txt", "x\n");
    assert_ne!(clean, tree_digest(repo.path()).unwrap());
}

#[test]
fn different_contents_of_the_same_path_differ() {
    let repo = repository();
    write(repo.path(), "new.txt", "x\n");
    let first = tree_digest(repo.path()).unwrap();
    write(repo.path(), "new.txt", "y\n");
    assert_ne!(first, tree_digest(repo.path()).unwrap());
}

#[test]
fn a_rename_changes_the_digest() {
    let repo = repository();
    write(repo.path(), "new.txt", "x\n");
    let before = tree_digest(repo.path()).unwrap();
    std::fs::rename(repo.path().join("new.txt"), repo.path().join("moved.txt")).unwrap();
    assert_ne!(before, tree_digest(repo.path()).unwrap());
}

#[test]
fn a_deleted_tracked_file_changes_the_digest() {
    let repo = repository();
    let clean = tree_digest(repo.path()).unwrap();
    std::fs::remove_file(repo.path().join("a.txt")).unwrap();
    assert_ne!(clean, tree_digest(repo.path()).unwrap());
}

#[test]
fn an_ignored_file_does_not_change_the_digest() {
    let repo = repository();
    let clean = tree_digest(repo.path()).unwrap();
    write(repo.path(), "build.log", "noise\n");
    assert_eq!(clean, tree_digest(repo.path()).unwrap());
}

#[test]
fn sandbox_mount_points_do_not_change_the_digest() {
    let repo = repository();
    let clean = tree_digest(repo.path()).unwrap();
    write(repo.path(), ".bashrc", "");
    write(repo.path(), ".mcp.json", "");
    assert_eq!(clean, tree_digest(repo.path()).unwrap());
}

#[test]
fn the_knowledge_digest_ignores_files_outside_the_knowledge_directory() {
    let repo = repository();
    let clean = knowledge_tree_digest(repo.path()).unwrap();
    write(repo.path(), "elsewhere.txt", "x\n");
    write(repo.path(), "a.txt", "changed\n");
    assert_eq!(clean, knowledge_tree_digest(repo.path()).unwrap());
    write(repo.path(), "doc/loom/knowledge/note.md", "changed\n");
    assert_ne!(clean, knowledge_tree_digest(repo.path()).unwrap());
}
