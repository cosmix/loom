use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use super::{announce_ready, await_go, Control, GO, READY};
use crate::native_worker::protocol::wire::SandboxMode;

const SHORT: Duration = Duration::from_millis(200);

fn pair() -> (Control, UnixStream) {
    let (supervisor, bridge) = UnixStream::pair().expect("a socket pair");
    (Control::new(supervisor), bridge)
}

#[test]
fn an_unconfirmed_launch_cannot_start_the_protocol() {
    let (mut control, mut bridge) = pair();
    let error = control.activate().expect_err("no ready message was read");
    assert!(format!("{error:#}").contains("unconfirmed"), "{error:#}");
    bridge
        .set_read_timeout(Some(SHORT))
        .expect("bound the read");
    let mut byte = [0u8; 1];
    assert!(
        bridge.read(&mut byte).is_err(),
        "the bridge must not be released"
    );
}

#[test]
fn a_bridge_that_says_something_else_stays_unconfirmed() {
    let (mut control, mut bridge) = pair();
    bridge.write_all(b"HELLO\n").expect("write");
    assert!(control.await_ready(SHORT).is_err());
    assert!(control.activate().is_err());
}

#[test]
fn a_silent_bridge_times_out_and_stays_unconfirmed() {
    let (mut control, _bridge) = pair();
    let error = control.await_ready(SHORT).expect_err("nothing was sent");
    assert!(format!("{error:#}").contains("within"), "{error:#}");
    assert!(control.activate().is_err());
}

#[test]
fn a_bridge_that_exits_early_stays_unconfirmed() {
    let (mut control, bridge) = pair();
    drop(bridge);
    let error = control.await_ready(SHORT).expect_err("the bridge is gone");
    assert!(format!("{error:#}").contains("exited"), "{error:#}");
    assert!(control.activate().is_err());
}

#[test]
fn a_confirmed_bridge_is_released_once_with_danger_full_access() {
    let (mut control, mut bridge) = pair();
    announce_ready(&mut bridge).expect("ready");
    control.await_ready(SHORT).expect("confirmed");
    assert_eq!(
        control.activate().expect("released"),
        SandboxMode::DangerFullAccess
    );
    await_go(&mut bridge).expect("the bridge reads its release");
    assert!(control.activate().is_err(), "a second release is refused");
}

#[test]
fn the_bridge_refuses_to_start_when_the_supervisor_closes_first() {
    let (control, mut bridge) = pair();
    drop(control);
    let error = await_go(&mut bridge).expect_err("no release");
    assert!(format!("{error:#}").contains("closed"), "{error:#}");
}

#[test]
fn the_messages_are_distinct_lines() {
    assert_ne!(&READY[..], &GO[..]);
    assert!(READY.ends_with(b"\n") && GO.ends_with(b"\n"));
}
