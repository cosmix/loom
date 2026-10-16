use super::*;
use crate::process::lifecycle_tests::InheritorGuard;
use std::os::unix::fs::PermissionsExt;

fn parent_exit_fixture(reply: bool) {
    let temp = tempfile::tempdir().unwrap();
    let guard = InheritorGuard::new(temp.path());
    let executable = temp.path().join("fake-codex");
    let response = if reply {
        "echo '{\"id\":1,\"result\":{}}'"
    } else {
        "true"
    };
    std::fs::write(
        &executable,
        format!("#!/bin/sh\n{} {response}\n", guard.shell()),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let start = Instant::now();
    let result = poll_once(
        &executable,
        Duration::from_secs(10),
        &AtomicBool::new(false),
        0,
    );
    assert!(start.elapsed() < Duration::from_secs(4));
    assert_eq!(result.is_ok(), reply, "unexpected reply: {result:?}");
    guard.assert_retired();
}

#[test]
fn leader_exit_with_stdout_inheritor_and_reply_is_bounded() {
    parent_exit_fixture(true);
}

#[test]
fn leader_exit_with_stdout_inheritor_without_reply_is_bounded() {
    parent_exit_fixture(false);
}
