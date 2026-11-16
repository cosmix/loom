//! Supervisor tests: settlement, the loop against a scripted peer, the
//! codex-home preflight, the pipe transport and boundary teardown.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::{json, Value};

use super::io::{CappedLog, PipeTransport};
use super::{run_session, settle, teardown, Clock};
use crate::native_worker::boundary::{identity_for, spawn_payload, verify_quiescent, Quiescence};
use crate::native_worker::index::{stage_disposition, unit_state, NativeDisposition};
use crate::native_worker::model::{
    CancelRequest, ControlMessage, Delivery, MessageState, NativeEvent, Outcome, Ownership,
    MAX_FRAME_BYTES,
};
use crate::native_worker::protocol::frame::{read_frame, FrameError, Transport};
use crate::native_worker::store::{self, read_events};
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};

// The frozen contract file declares the same harness privately in its own module, so these
// tests load a second copy; revisit if the harness gains a shared declaration.
#[allow(clippy::duplicate_mod)]
#[path = "../testkit/mod.rs"]
mod testkit;

use testkit::peer::{agent_message, line, turn_completed};
use testkit::{event_lines, invocation, End, ScriptedPeer, SteppingClock, STAGE, TURN, UNIT};

fn events(dir: &Path) -> Vec<NativeEvent> {
    read_events(dir)
        .expect("events")
        .into_iter()
        .map(|record| record.event)
        .collect()
}

/// A scripted peer plus a hook that runs on every frame the loop sends and
/// may queue raw lines, delivered before the peer's own.
struct Hooked {
    peer: ScriptedPeer,
    dir: PathBuf,
    hook: fn(&Path, &Value) -> Vec<Vec<u8>>,
    extra: VecDeque<Vec<u8>>,
}

impl Transport for Hooked {
    fn send(&mut self, frame: &Value) -> io::Result<()> {
        self.peer.send(frame)?;
        self.extra.extend((self.hook)(&self.dir, frame));
        Ok(())
    }

    fn recv(&mut self, timeout: Duration) -> Result<Option<Value>, FrameError> {
        match self.extra.pop_front() {
            Some(raw) => read_frame(&mut Cursor::new(raw), MAX_FRAME_BYTES).map(Some),
            None => self.peer.recv(timeout),
        }
    }
}

/// A peer through `turn/start` that then goes silent and arms `clock`.
fn hooked(dir: &Path, clock: &SteppingClock, hook: fn(&Path, &Value) -> Vec<Vec<u8>>) -> Hooked {
    let peer = ScriptedPeer::through_turn_start(Vec::new(), End::Silent).arming(clock);
    let (dir, extra) = (dir.to_path_buf(), VecDeque::new());
    Hooked {
        peer,
        dir,
        hook,
        extra,
    }
}

fn method(frame: &Value) -> Option<&str> {
    frame.get("method").and_then(Value::as_str)
}

/// Readings come from a list; the last one repeats.
struct ListClock(RefCell<VecDeque<Instant>>);

impl Clock for ListClock {
    fn now(&self) -> Instant {
        let mut readings = self.0.borrow_mut();
        let front = readings[0];
        if readings.len() > 1 {
            readings.pop_front();
        }
        front
    }
}

#[test]
#[serial_test::serial]
fn settle_released_records_terminal_then_release() {
    let record = invocation();
    let reason = "no codex";
    settle(&record.dir, Outcome::Failed, reason, Quiescence::Released).expect("settle");
    let tail = &events(&record.dir)[1..];
    assert!(
        matches!(&tail[0], NativeEvent::Terminal { outcome: Outcome::Failed, reason } if reason == "no codex")
    );
    assert!(matches!(&tail[1], NativeEvent::OwnershipReleased { by } if by == "supervisor"));
    let state = unit_state(&record.work, STAGE, UNIT)
        .unwrap()
        .expect("unit");
    assert_eq!(state.outcome, Some(Outcome::Failed));
    assert_eq!(state.ownership, Ownership::Released);
}

#[test]
#[serial_test::serial]
fn settle_failed_with_unverified_keeps_outcome() {
    let record = invocation();
    let quiescence = Quiescence::Unverified("members remain".into());
    settle(&record.dir, Outcome::Failed, "turn failed", quiescence).expect("settle");
    let state = unit_state(&record.work, STAGE, UNIT)
        .unwrap()
        .expect("unit");
    assert_eq!(state.outcome, Some(Outcome::Failed));
    assert_eq!(state.ownership, Ownership::Unverified);
    let disposition = stage_disposition(&record.work, STAGE);
    assert!(
        matches!(disposition, NativeDisposition::Unverified(_)),
        "{disposition:?}"
    );
}

fn cancel_on_turn_start(dir: &Path, frame: &Value) -> Vec<Vec<u8>> {
    if method(frame) == Some("turn/start") {
        let request = CancelRequest {
            reason: "operator stop".into(),
            requested_at: Utc::now(),
            by: "test".into(),
        };
        store::write_cancel(dir, &request).expect("write cancel");
    }
    Vec::new()
}

#[test]
#[serial_test::serial]
fn cancel_file_cancels_an_active_turn() {
    let record = invocation();
    let clock = SteppingClock::new(&[Duration::from_millis(150)]);
    let mut transport = hooked(&record.dir, &clock, cancel_on_turn_start);
    let (outcome, reason) = run_session(&record.dir, &mut transport, &clock).expect("run");
    assert_eq!(outcome, Outcome::Cancelled, "reason: {reason}");
    assert!(transport.peer.sent_methods().contains(&"turn/interrupt"));
    let cancelled = events(&record.dir)
        .iter()
        .any(|event| matches!(event, NativeEvent::CancelRequested { reason } if reason == "operator stop"));
    assert!(cancelled, "no CancelRequested event");
}

fn steer_on_turn_start(dir: &Path, frame: &Value) -> Vec<Vec<u8>> {
    match method(frame) {
        Some("turn/start") => {
            let message = ControlMessage {
                message_id: "steer-1".into(),
                delivery: Delivery::Steer,
                text: "also add a test".into(),
                sha256: crate::utils::sha256_hex(b"also add a test"),
                accepted_at: Utc::now(),
            };
            store::write_message(dir, &message).expect("write message");
            Vec::new()
        }
        Some("turn/steer") => vec![
            line(&json!({ "id": frame["id"], "result": { "turnId": TURN } })),
            agent_message("m1", "Done.", "final_answer"),
            turn_completed("completed"),
        ],
        _ => Vec::new(),
    }
}

#[test]
#[serial_test::serial]
fn steer_message_is_sent_with_the_active_turn_id() {
    let record = invocation();
    let clock = SteppingClock::new(&[Duration::from_millis(150)]);
    let mut transport = hooked(&record.dir, &clock, steer_on_turn_start);
    let (outcome, reason) = run_session(&record.dir, &mut transport, &clock).expect("run");
    assert_eq!(outcome, Outcome::Succeeded, "reason: {reason}");
    let sent = &transport.peer.sent;
    let steer = sent
        .iter()
        .find(|f| method(f) == Some("turn/steer"))
        .expect("no steer sent");
    assert_eq!(steer["params"]["expectedTurnId"], TURN);
    let states: Vec<MessageState> = events(&record.dir)
        .into_iter()
        .filter_map(|event| match event {
            NativeEvent::Message { state, .. } => Some(state),
            _ => None,
        })
        .collect();
    assert_eq!(states, [MessageState::Dispatching, MessageState::Delivered]);
}

fn noise(i: u32) -> Vec<u8> {
    line(&json!({ "method": "noise/unknown", "params": { "i": i } }))
}

#[test]
#[serial_test::serial]
fn progress_events_are_throttled() {
    let record = invocation();
    let t0 = Instant::now();
    // Readings: loop start, three handshake iterations, then one per frame.
    let offsets = [0, 0, 0, 0, 1, 2, 31, 31];
    let readings = offsets
        .iter()
        .map(|s| t0 + Duration::from_secs(*s))
        .collect();
    let clock = ListClock(RefCell::new(readings));
    let mut peer = ScriptedPeer::through_turn_start(vec![noise(1), noise(2), noise(3)], End::Close);
    let (outcome, _) = run_session(&record.dir, &mut peer, &clock).expect("run");
    assert_eq!(outcome, Outcome::Failed);
    let progress = events(&record.dir)
        .iter()
        .filter(|event| matches!(event, NativeEvent::Progress))
        .count();
    assert_eq!(progress, 1);
}

#[test]
#[serial_test::serial]
fn event_log_not_writable_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    if crate::models::forward_receipt::locator::effective_uid() == 0 {
        return;
    }
    let record = invocation();
    let path = record.dir.join("events.jsonl");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).expect("chmod");
    let mut peer = ScriptedPeer::through_turn_start(Vec::new(), End::Close);
    let clock = SteppingClock::new(&[]);
    let result = run_session(&record.dir, &mut peer, &clock);
    assert!(result.is_err(), "run_session returned {result:?}");
}

#[test]
#[serial_test::serial]
fn unknown_and_oversized_frames_do_not_end_the_session() {
    let record = invocation();
    let lines = vec![
        noise(1),
        line(&json!({ "method": "noise/huge", "params": "x".repeat(MAX_FRAME_BYTES + 1) })),
        b"not json\n".to_vec(),
        agent_message("m1", "All done.", "final_answer"),
        turn_completed("completed"),
    ];
    let mut peer = ScriptedPeer::through_turn_start(lines, End::Silent);
    let clock = SteppingClock::new(&[]);
    let (outcome, reason) = run_session(&record.dir, &mut peer, &clock).expect("run");
    assert_eq!(outcome, Outcome::Succeeded, "reason: {reason}");
    assert_eq!(event_lines(&record.dir), events(&record.dir).len());
}

/// A spawn that failed once bubblewrap was started has no identity to prove
/// its namespace empty: it settles `Failed` with ownership `Unverified`.
#[test]
#[serial_test::serial]
fn spawn_failure_after_start_settles_unverified() {
    let record = invocation();
    let code = teardown::settle_spawn_failure(&record.dir, "info fd timed out", true);
    assert_eq!(code, std::process::ExitCode::SUCCESS);
    let tail = &events(&record.dir)[1..];
    assert!(
        matches!(&tail[0], NativeEvent::Terminal { outcome: Outcome::Failed, reason }
        if reason == "boundary spawn failed: info fd timed out")
    );
    assert!(
        matches!(&tail[1], NativeEvent::OwnershipUnverified { detail }
        if detail.contains("info fd timed out"))
    );
    let state = unit_state(&record.work, STAGE, UNIT)
        .unwrap()
        .expect("unit");
    assert_eq!(state.ownership, Ownership::Unverified);
}

/// Only a spawn refused before bubblewrap ran records `Released` unchecked.
#[test]
#[serial_test::serial]
fn spawn_failure_before_start_settles_released() {
    let record = invocation();
    teardown::settle_spawn_failure(&record.dir, "a symlinked .loom", false);
    let tail = &events(&record.dir)[1..];
    assert!(matches!(&tail[1], NativeEvent::OwnershipReleased { by } if by == "supervisor"));
}

#[test]
fn capped_log_stops_at_the_cap_with_one_marker() {
    let dir = tempfile::TempDir::new().expect("temp");
    let mut log = CappedLog::open_with_cap(dir.path(), 100).expect("open");
    for i in 0..20 {
        log.write_line(&format!("{{\"frame\":{i}}}"))
            .expect("write");
    }
    let text = fs::read_to_string(dir.path().join("provider.log")).expect("read");
    let marker = r#"{"loom":"provider log capped"}"#;
    assert_eq!(text.matches(marker).count(), 1);
    assert!(text.ends_with(&format!("{marker}\n")));
    assert!(text.len() <= 100 + marker.len() + 1, "{} bytes", text.len());
}

fn sink() -> io::PipeWriter {
    let (reader, writer) = io::pipe().expect("pipe");
    std::thread::spawn(move || io::copy(&mut { reader }, &mut io::sink()));
    writer
}

#[test]
fn reader_reports_oversized_and_continues() {
    let dir = tempfile::TempDir::new().expect("temp");
    let (output, mut provider) = io::pipe().expect("pipe");
    let log = CappedLog::open(dir.path()).expect("log");
    let mut transport = PipeTransport::start(output, sink(), Some(log)).expect("start");
    let mut raw = vec![b'x'; 2 * 1024 * 1024];
    raw.extend_from_slice(b"\n{\"method\":\"ok\"}\n");
    let writer = std::thread::spawn(move || provider.write_all(&raw).expect("write"));
    let wait = Duration::from_secs(10);
    assert!(matches!(transport.recv(wait), Err(FrameError::Oversized)));
    let frame = transport.recv(wait).expect("frame").expect("not a timeout");
    assert_eq!(frame, json!({ "method": "ok" }));
    writer.join().expect("writer");
    assert!(matches!(transport.recv(wait), Err(FrameError::Eof)));
    transport.close_input().join();
    let logged = fs::read_to_string(dir.path().join("provider.log")).expect("log");
    let expected = "{\"loom\":\"oversized frame\"}\n{\"method\":\"ok\"}\n";
    assert_eq!(logged, expected);
}

#[test]
fn recv_times_out_with_none() {
    let (output, provider) = io::pipe().expect("pipe");
    let mut transport = PipeTransport::start(output, sink(), None).expect("start");
    let frame = transport.recv(Duration::from_millis(50)).expect("no error");
    assert!(frame.is_none());
    drop(provider);
    transport.close_input().join();
}

#[test]
fn send_after_the_reader_closed_is_an_error() {
    let (output, provider) = io::pipe().expect("pipe");
    let (stdin_reader, stdin) = io::pipe().expect("pipe");
    drop(stdin_reader);
    let mut transport = PipeTransport::start(output, stdin, None).expect("start");
    let frame = json!({ "method": "initialized", "params": {} });
    let started = Instant::now();
    let error = loop {
        match transport.send(&frame) {
            Err(error) => break error,
            Ok(()) if started.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(()) => panic!("send never failed after the reader closed"),
        }
    };
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    drop(provider);
    transport.close_input().join();
}

#[test]
#[serial_test::serial]
fn stop_process_ends_a_sleeping_boundary_and_namespace_is_released() {
    let name = "stop_process_ends_a_sleeping_boundary_and_namespace_is_released";
    if skip_unless(confined_commands_runnable(), name, "needs bubblewrap") {
        return;
    }
    let tmp = tempfile::TempDir::new().expect("temp");
    let worktree = tmp.path().canonicalize().expect("canonical");
    let args = [OsString::from("-c"), OsString::from("sleep 30")];
    let mut boundary = spawn_payload(&worktree, Path::new("/bin/sh"), &args).expect("spawn");
    let identity = identity_for(&boundary).expect("identity");
    teardown::stop_process(&mut boundary.process);
    assert_eq!(verify_quiescent(&identity), Quiescence::Released);
    assert!(boundary.process.try_wait().expect("try_wait").is_some());
}
