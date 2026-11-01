//! The pass-cache fingerprint reads a stage worktree through its registered
//! git directory, whatever the worktree's `.git` file names: the host child
//! computes it outside every sandbox.

use serial_test::serial;
use tempfile::TempDir;

use super::{capture, digest_bounded, digest_regular_file, Digest, ExecutionIdentity, Sha256};
use crate::git::worktree::WorktreeGit;
use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use crate::verify::criteria::confine::{prepare_confined, CommandSpec};

struct GrowingReader {
    read: usize,
}

impl std::io::Read for GrowingReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        buf.fill(b'x');
        self.read += buf.len();
        Ok(buf.len())
    }
}

#[test]
fn growing_stream_stops_after_one_detection_byte() {
    let mut reader = GrowingReader { read: 0 };
    assert!(digest_bounded(&mut reader, 65_536, Sha256::new()).is_none());
    assert_eq!(reader.read, 65_537);
}

#[test]
fn regular_file_digest_preserves_blob_header_and_budget() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("blob");
    std::fs::write(&path, b"hello").unwrap();
    let metadata = path.metadata().unwrap();
    let mut hasher = sha1::Sha1::new();
    hasher.update(b"blob 5\0");
    let mut remaining = 8;
    let digest = digest_regular_file(&path, &metadata, &mut remaining, hasher).unwrap();
    assert_eq!(
        hex::encode(digest),
        "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0"
    );
    assert_eq!(remaining, 3);
    let digest = digest_regular_file(&path, &metadata, &mut 5, Sha256::new()).unwrap();
    assert_eq!(digest, Sha256::digest(b"hello"));
    let mut hasher = Sha256::new();
    hasher.update(b"blob 5\0");
    let digest = digest_regular_file(&path, &metadata, &mut 5, hasher).unwrap();
    assert_eq!(
        hex::encode(digest),
        "8aec4e4876f854f688d0ebfc8f37598f38e5fd6903cccc850ca36591175aeb60"
    );
}

#[test]
fn file_grown_after_snapshot_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("blob");
    std::fs::write(&path, b"hello").unwrap();
    let metadata = path.metadata().unwrap();
    std::fs::write(&path, b"hello world").unwrap();
    assert!(digest_regular_file(&path, &metadata, &mut 5, Sha256::new()).is_none());
}

#[test]
fn replaced_file_is_rejected_even_with_equal_length() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("blob");
    let replacement = dir.path().join("replacement");
    std::fs::write(&path, b"hello").unwrap();
    let metadata = path.metadata().unwrap();
    std::fs::write(&replacement, b"world").unwrap();
    std::fs::rename(replacement, &path).unwrap();
    assert!(digest_regular_file(&path, &metadata, &mut 5, Sha256::new()).is_none());
}

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
