use super::*;
use crate::process::lifecycle_tests::InheritorGuard;

fn parent_exit_fixture(reply: bool) {
    let temp = tempfile::tempdir().unwrap();
    let guard = InheritorGuard::new(temp.path());
    let response = if reply {
        "echo '{\"id\":1,\"result\":{}}'"
    } else {
        "true"
    };
    let mut command = Command::new("/bin/sh");
    command.args(["-c", &format!("{} {response}", guard.shell())]);
    let start = Instant::now();
    let result = poll_command(command, Duration::from_secs(10), &AtomicBool::new(false), 0);
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
