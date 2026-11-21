//! The startup deadline in the real loop, against scripted peers and a fake clock.

use std::io;
use std::time::Duration;

use serde_json::Value;

use super::run_session;
use crate::native_worker::model::{NativeEvent, Outcome};
use crate::native_worker::protocol::frame::{FrameError, Transport};
use crate::native_worker::store::read_events;

// The record fixture is shared with the other runtime tests, which declare the same
// harness privately.
#[allow(clippy::duplicate_mod, dead_code, unused_imports)]
#[path = "../testkit/mod.rs"]
mod testkit;

use testkit::{invocation, End, ScriptedPeer, SteppingClock};

const REASON: &str = "startup deadline (120 s) passed before the Codex thread started";

/// A peer that accepts every frame and never answers.
struct SilentPeer {
    sent: Vec<Value>,
}

impl Transport for SilentPeer {
    fn send(&mut self, frame: &Value) -> io::Result<()> {
        self.sent.push(frame.clone());
        Ok(())
    }

    fn recv(&mut self, _timeout: Duration) -> Result<Option<Value>, FrameError> {
        Ok(None)
    }
}

fn thread_started(dir: &std::path::Path) -> bool {
    read_events(dir)
        .expect("events")
        .iter()
        .any(|record| matches!(record.event, NativeEvent::ThreadStarted { .. }))
}

#[test]
#[serial_test::serial]
fn a_peer_that_never_answers_initialize_times_out_at_the_startup_deadline() {
    let record = invocation();
    // The first reading is the start; every later one advances 30 s, so the
    // fourth reading after it reaches 120 s.
    let clock = SteppingClock::new(&[Duration::ZERO, Duration::from_secs(30)]);
    clock.arm();
    let mut peer = SilentPeer { sent: Vec::new() };
    let (outcome, reason) = run_session(
        &record.dir,
        &mut peer,
        &clock,
        crate::native_worker::protocol::wire::SandboxMode::WorkspaceWrite,
    )
    .expect("run_session");
    assert_eq!((outcome, reason.as_str()), (Outcome::TimedOut, REASON));
    let methods: Vec<_> = peer
        .sent
        .iter()
        .filter_map(|frame| frame.get("method").and_then(Value::as_str))
        .collect();
    assert_eq!(methods, ["initialize"]);
    assert!(!thread_started(&record.dir));
}

#[test]
#[serial_test::serial]
fn a_handshake_frame_queued_past_the_deadline_is_not_accepted() {
    let record = invocation();
    // One reading at the start, then one jump past 120 s before anything is received.
    let clock = SteppingClock::new(&[Duration::ZERO, Duration::from_secs(130)]);
    clock.arm();
    let mut peer = ScriptedPeer::through_turn_start(Vec::new(), End::Close);
    let (outcome, reason) = run_session(
        &record.dir,
        &mut peer,
        &clock,
        crate::native_worker::protocol::wire::SandboxMode::WorkspaceWrite,
    )
    .expect("run_session");
    assert_eq!((outcome, reason.as_str()), (Outcome::TimedOut, REASON));
    assert_eq!(peer.sent_methods(), ["initialize"]);
    assert!(!thread_started(&record.dir));
}

#[test]
#[serial_test::serial]
fn a_thread_started_in_time_disarms_the_startup_deadline() {
    let record = invocation();
    // 10 s per reading: the handshake finishes within 120 s, then the clock
    // runs far past it. The unit deadline, not startup, ends the run.
    let clock = SteppingClock::new(&[Duration::ZERO, Duration::from_secs(10)]);
    clock.arm();
    let mut peer = ScriptedPeer::through_turn_start(Vec::new(), End::Silent);
    let (outcome, reason) = run_session(
        &record.dir,
        &mut peer,
        &clock,
        crate::native_worker::protocol::wire::SandboxMode::WorkspaceWrite,
    )
    .expect("run_session");
    assert_eq!(
        (outcome, reason.as_str()),
        (Outcome::TimedOut, "deadline expired")
    );
    assert!(thread_started(&record.dir));
}
