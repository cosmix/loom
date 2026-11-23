use super::*;
use crate::git::merge::test_support::{init_repo, lock_dir};
use crate::git::merge::{merge_stage, MergeBlock, MergeGate, MergeResult};
use crate::test_git::{commit_file, git};
use std::os::unix::fs::symlink;

#[test]
fn the_probe_reads_the_disk_and_compares_bytes() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "blob.txt", "same", "blob");
    std::fs::create_dir(root.join("dir")).unwrap();
    symlink("a.txt", root.join("link")).unwrap();
    let probe = DiskProbe {
        repo: root,
        base: "HEAD~1",
        tree: "HEAD",
    };

    assert_eq!(probe.disk("blob.txt"), DiskEntry::RegularFile);
    assert_eq!(probe.disk("dir"), DiskEntry::Directory);
    assert_eq!(probe.disk("link"), DiskEntry::Other);
    assert_eq!(probe.disk("missing"), DiskEntry::Absent);
    assert_eq!(probe.disk("a.txt/below"), DiskEntry::Absent);
    assert!(probe.equals_blob("blob.txt"));
    std::fs::write(root.join("blob.txt"), "different").unwrap();
    assert!(!probe.equals_blob("blob.txt"));
    assert!(!probe.equals_blob("link"), "a symlink never equals a blob");
    assert!(probe.in_base_tree("a.txt"));
    assert!(!probe.in_base_tree("blob.txt"));
}

#[test]
fn a_symlinked_ignored_parent_blocks_and_the_outside_file_survives() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, ".gitignore", "a\n", "ignore a");
    git(root, &["checkout", "-b", "loom/s1", "main"]);
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::write(root.join("a/b/f"), "incoming").unwrap();
    git(root, &["add", "-f", "a/b/f"]);
    git(root, &["commit", "-m", "stage a/b/f"]);
    git(root, &["checkout", "main"]);
    if root.join("a").exists() {
        std::fs::remove_dir_all(root.join("a")).unwrap();
    }
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(outside.path().join("b")).unwrap();
    std::fs::write(outside.path().join("b/f"), "incoming").unwrap();
    symlink(outside.path(), root.join("a")).unwrap();
    let probe = DiskProbe {
        repo: root,
        base: "main",
        tree: "loom/s1",
    };
    assert_eq!(probe.disk("a/b/f"), DiskEntry::Other);
    assert!(!probe.equals_blob("a/b/f"));
    let work = lock_dir();

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap();

    assert!(
        matches!(
            result,
            MergeResult::Blocked(MergeBlock::UncommittedOverlap { .. })
        ),
        "expected UncommittedOverlap, got {result:?}"
    );
    assert_eq!(
        std::fs::read(outside.path().join("b/f")).unwrap(),
        b"incoming"
    );
}
