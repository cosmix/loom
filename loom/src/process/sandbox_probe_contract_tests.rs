//! Contract: `start_blocker` runs the launcher's trivial start and reports a
//! refusal with the launcher's exit status and its first non-empty stderr line.

use crate::process::sandbox_probe::start_blocker;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const REFUSAL: &str = "sandbox-exec: sandbox_apply: Operation not permitted";

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write script");
    let mut permissions = std::fs::metadata(&path).expect("stat script").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod script");
    path
}

#[test]
fn a_refused_sandbox_start_is_a_blocker() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let refused = script(
        dir.path(),
        "refused.sh",
        &format!("echo '' >&2\necho '{REFUSAL}' >&2\necho 'second line' >&2\nexit 71"),
    );
    let started = script(
        dir.path(),
        "started.sh",
        "[ \"$1\" = --flag ] && [ \"$2\" = value ] || exit 3\nexit 0",
    );

    let reason = start_blocker(&refused, &[]).expect("a launcher exiting 71 is a blocker");
    assert!(reason.contains(REFUSAL), "stderr line missing: {reason}");
    assert!(reason.contains("71"), "exit status missing: {reason}");
    assert!(
        !reason.contains("second line"),
        "only the first line: {reason}"
    );

    assert_eq!(start_blocker(&started, &["--flag", "value"]), None);
    assert!(
        start_blocker(&started, &[]).is_some(),
        "the arguments must reach the launcher"
    );

    let missing = dir.path().join("no-such-launcher");
    assert!(
        start_blocker(&missing, &[]).is_some(),
        "a launcher that cannot be spawned is a blocker"
    );
}

#[test]
fn a_launcher_that_never_finishes_is_a_blocker_and_is_reaped() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let hung = script(dir.path(), "hung.sh", "exec sleep 600");
    let started = std::time::Instant::now();
    let reason = start_blocker(&hung, &[]).expect("a launcher that hangs is a blocker");
    assert!(
        reason.contains("did not finish"),
        "deadline missing: {reason}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "the wait must be bounded"
    );
}
