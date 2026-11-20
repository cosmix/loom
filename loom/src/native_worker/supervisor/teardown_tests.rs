//! Teardown of a boundary whose identity could not be recorded.
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::anyhow;

use super::{settle_unrecorded, stop_unrecorded};
use crate::native_worker::boundary::{kill_namespace, spawn_payload, Quiescence, SpawnedBoundary};
use crate::native_worker::model::{NativeEvent, Outcome};
use crate::native_worker::store::read_events;
use crate::process::sandbox_probe::{
    confined_commands_runnable, skip_unless, skip_without_group_signals,
};
use crate::process::OwnedProcess;
use crate::verify::criteria::pid_namespace::members;

/// A boundary running `sleep 30` in `worktree`, with its real bubblewrap held
/// aside (returned) and a stand-in in its place, so retiring the boundary's
/// process neither kills the namespace's first process nor lets
/// `--die-with-parent` take the namespace down.
fn held_boundary(worktree: &Path) -> (SpawnedBoundary, OwnedProcess) {
    let args = [OsString::from("-c"), OsString::from("sleep 30")];
    let mut boundary = spawn_payload(worktree, Path::new("/bin/sh"), &args).expect("spawn");
    let stand_in = Command::new("true").process_group(0).spawn().expect("true");
    let real = std::mem::replace(&mut boundary.process, OwnedProcess::new(stand_in));
    (boundary, real)
}

fn no_boundary(name: &str) -> bool {
    skip_unless(confined_commands_runnable(), name, "needs bubblewrap")
        || skip_without_group_signals(name)
}

/// Without an identity the namespace is still killed by the pid and inode
/// bubblewrap reported; proven empty, it releases the scratch. The real
/// bubblewrap is alive throughout, so only that kill can empty it.
#[test]
#[serial_test::serial]
fn an_unrecorded_boundary_is_emptied_and_its_scratch_released() {
    if no_boundary("an_unrecorded_boundary_is_emptied_and_its_scratch_released") {
        return;
    }
    let tmp = tempfile::TempDir::new().expect("temp");
    let worktree = tmp.path().canonicalize().expect("canonical");
    let (boundary, real) = held_boundary(&worktree);
    let namespace = boundary.pid_namespace;
    let scratch = boundary.scratch.path().to_path_buf();
    let quiescence = stop_unrecorded(boundary);
    let left = members(namespace);
    drop(real);
    assert_eq!(quiescence, Quiescence::Released);
    assert_eq!(left, Ok(Vec::new()));
    assert!(!scratch.exists(), "{} survived", scratch.display());
}

/// A namespace the kill cannot prove empty (its first process misreported)
/// keeps the scratch, and the settlement is `Unverified` with the identity
/// error ahead of the members left.
#[test]
#[serial_test::serial]
fn an_unrecorded_boundary_not_proven_empty_keeps_its_scratch() {
    if no_boundary("an_unrecorded_boundary_not_proven_empty_keeps_its_scratch") {
        return;
    }
    let tmp = tempfile::TempDir::new().expect("temp");
    let worktree = tmp.path().canonicalize().expect("canonical");
    let dir = tmp.path().join("invocation");
    std::fs::create_dir(&dir).expect("invocation dir");
    std::fs::write(dir.join("events.jsonl"), "").expect("event log");
    let (mut boundary, real) = held_boundary(&worktree);
    let (first, namespace) = (boundary.namespace_pid, boundary.pid_namespace);
    let scratch = boundary.scratch.path().to_path_buf();
    // This process is no member, so the kill signals nothing.
    boundary.namespace_pid = std::process::id() as i32;
    let error = anyhow!("no supervisor start time");
    let code = settle_unrecorded(&dir, boundary, &error);
    let kept = scratch.is_dir();
    let cleaned = kill_namespace(first, namespace);
    drop(real);
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(code, std::process::ExitCode::SUCCESS);
    assert!(kept, "the scratch of an unproven namespace was removed");
    assert_eq!(cleaned, Quiescence::Released);
    let events: Vec<_> = read_events(&dir)
        .unwrap()
        .into_iter()
        .map(|r| r.event)
        .collect();
    assert!(
        matches!(&events[0], NativeEvent::Terminal { outcome: Outcome::Unknown, reason }
            if reason == "could not record boundary: no supervisor start time"),
        "{events:?}"
    );
    let NativeEvent::OwnershipUnverified { detail } = &events[1] else {
        panic!("{events:?}");
    };
    let prefix = "no boundary identity (no supervisor start time): ";
    assert!(detail.starts_with(prefix), "{detail}");
    assert!(
        detail.contains(&format!("in pid namespace {namespace}")),
        "{detail}"
    );
}
