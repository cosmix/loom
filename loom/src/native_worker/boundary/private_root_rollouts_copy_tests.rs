//! The copy a move across devices makes: streamed through a partial file
//! that never stays in the host tree, and compared block by block. Called
//! directly, since one fixture filesystem cannot make `linkat` cross devices.
use std::ffi::OsString;
use std::fs::{self, File};
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};

use super::{copy_across, same_bytes, Rollout, CHUNK};

const NAME: &str = "rollout-2026-11-21T10-00-00-thread-c.jsonl";

struct Trees {
    _dir: tempfile::TempDir,
    private: PathBuf,
    host: PathBuf,
}

fn trees(contents: &[u8]) -> Trees {
    let dir = tempfile::tempdir().expect("a temp dir");
    let private = dir.path().join("private");
    let host = dir.path().join("host");
    fs::create_dir_all(&private).expect("the private tree");
    fs::create_dir_all(&host).expect("the host tree");
    fs::write(private.join(NAME), contents).expect("a rollout");
    Trees {
        _dir: dir,
        private,
        host,
    }
}

fn rollout(private: &Path) -> Rollout {
    Rollout {
        dir: OwnedFd::from(File::open(private).expect("the private dir")),
        name: OsString::from(NAME),
        relative: PathBuf::new(),
    }
}

fn copy(trees: &Trees) -> std::io::Result<()> {
    let target_dir = File::open(&trees.host).expect("the host dir");
    copy_across(
        &rollout(&trees.private),
        target_dir.as_fd(),
        &trees.host.join(NAME),
    )
}

fn host_names(trees: &Trees) -> Vec<OsString> {
    let entries = fs::read_dir(&trees.host).expect("the host dir");
    entries
        .map(|entry| entry.expect("an entry").file_name())
        .collect()
}

/// More than three blocks, so the copy and the comparison both loop.
fn large() -> Vec<u8> {
    (0..3 * CHUNK + 5)
        .map(|index| (index % 251) as u8)
        .collect()
}

#[test]
fn a_copy_across_places_the_whole_rollout_and_leaves_no_partial_file() {
    let contents = large();
    let trees = trees(&contents);
    copy(&trees).expect("copied");
    assert_eq!(fs::read(trees.host.join(NAME)).expect("placed"), contents);
    assert_eq!(host_names(&trees), [OsString::from(NAME)]);
    assert!(trees.private.join(NAME).exists(), "the caller unlinks it");
}

#[test]
fn a_copy_onto_another_file_keeps_it_and_leaves_no_partial_file() {
    let trees = trees(&large());
    fs::write(trees.host.join(NAME), b"other\n").expect("a taken name");
    let error = copy(&trees).expect_err("a taken name is refused");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(trees.host.join(NAME)).expect("kept"), b"other\n");
    assert_eq!(host_names(&trees), [OsString::from(NAME)]);
}

#[test]
fn a_copy_onto_the_same_bytes_counts_as_placed() {
    let contents = large();
    let trees = trees(&contents);
    fs::write(trees.host.join(NAME), &contents).expect("an earlier copy");
    copy(&trees).expect("the same bytes count as placed");
    assert_eq!(host_names(&trees), [OsString::from(NAME)]);
}

#[test]
fn files_differing_only_in_the_last_block_are_not_the_same() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let mut changed = large();
    *changed.last_mut().expect("bytes") ^= 1;
    for (name, contents) in [("a", large()), ("b", large()), ("c", changed)] {
        fs::write(dir.path().join(name), contents).expect("a file");
    }
    let open = |name: &str| File::open(dir.path().join(name)).expect("a file");
    assert!(same_bytes(open("a"), open("b")).expect("compared"));
    assert!(!same_bytes(open("a"), open("c")).expect("compared"));
}
