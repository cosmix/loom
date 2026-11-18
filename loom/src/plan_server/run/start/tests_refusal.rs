//! What a `blocked` refusal tells the operator to do.

use tempfile::TempDir;

use super::tests::state;
use super::*;
use crate::plan_server::wire::run::ChangedPath;

fn change(path: &str, status: &str) -> ChangedPath {
    ChangedPath {
        path: path.into(),
        status: status.into(),
    }
}

#[test]
fn tracked_changes_get_the_commit_or_stash_advice() {
    let root = TempDir::new().unwrap();
    let mut state = state();
    state.blocking_changes.push(change("src/lib.rs", " M"));

    assert_eq!(
        blocked_message(root.path(), &state),
        "commit or stash tracked changes outside doc/ before starting"
    );
}

#[test]
fn the_run_state_entry_is_refused_with_its_own_status() {
    let root = TempDir::new().unwrap();
    let status =
        "stage files remain; no plan names this run, so run `loom clean --all` in a terminal";
    let mut state = state();
    state.blocking_changes.push(change(".loom/work", status));

    assert_eq!(blocked_message(root.path(), &state), status);
}

#[test]
fn a_tracked_change_beside_the_run_state_entry_does_not_hide_its_remedy() {
    let root = TempDir::new().unwrap();
    let mut state = state();
    state.blocking_changes.push(change("src/lib.rs", " M"));
    state
        .blocking_changes
        .push(change(".loom/work", "worktrees remain"));

    assert_eq!(blocked_message(root.path(), &state), "worktrees remain");
}
