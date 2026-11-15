//! The eval refreshes the local source-graph snapshot once before any case
//! runs, and keeps going when that fails.

use super::{ensure_local_snapshot, refresh_source_graph};
use crate::context::freshness::GraphState;
use crate::context::refresh::SnapshotAction;
use crate::test_git::TestRepo;
use serial_test::serial;
use tempfile::TempDir;

/// A git repo, with one committed Rust file when `committed` and no HEAD at
/// all otherwise.
fn repo(committed: bool) -> TempDir {
    let builder = TestRepo::builder();
    if committed {
        builder.seed("src.rs", "pub fn refreshed() {}\n", "seed")
    } else {
        builder.no_seed()
    }
    .create()
    .into_temp_dir()
}

#[test]
#[serial]
fn the_local_snapshot_is_built_for_the_committed_tree() {
    let temp = repo(true);

    let outcome = ensure_local_snapshot(temp.path()).unwrap();

    assert_ne!(
        outcome.action,
        SnapshotAction::Unavailable,
        "{}",
        outcome.reason
    );
    assert_eq!(outcome.state(), GraphState::Current);
    assert!(!outcome.revision.is_empty());
}

#[test]
#[serial]
fn a_snapshot_that_cannot_be_built_is_reported_and_the_eval_goes_on() {
    let temp = repo(false);

    let outcome = ensure_local_snapshot(temp.path()).unwrap();
    assert_eq!(outcome.action, SnapshotAction::Unavailable);

    // Returns rather than raising: the cases still run against what exists.
    refresh_source_graph(temp.path());
}
