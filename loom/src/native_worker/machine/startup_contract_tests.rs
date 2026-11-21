//! Contract of stage codex-lane-lifetime: one monotonic startup deadline, armed at
//! `Input::Start`, spans `initialize` and `thread/start` in the real supervisor loop.

use std::cell::Cell;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::native_worker::machine::STARTUP_DEADLINE_SECS;
use crate::native_worker::model::{NativeEvent, Outcome, MAX_FRAME_BYTES, NATIVE_DEADLINE_SECS};
use crate::native_worker::protocol::frame::{read_frame, FrameError, Transport};
use crate::native_worker::store::read_events;
use crate::native_worker::supervisor::{run_session, Clock};

// The record fixture and frame encoder are shared with the other runtime tests,
// which declare the same harness privately.
#[allow(clippy::duplicate_mod, dead_code, unused_imports)]
#[path = "../testkit/mod.rs"]
mod testkit;

use testkit::invocation;
use testkit::peer::line;

const STARTUP_REASON: &str = "startup deadline (120 s) passed before the Codex thread started";
/// Fake seconds after which the transport reports end of stream, so a loop that
/// never times out still ends quickly instead of polling forever.
const CAP_SECS: u64 = 4_000;

/// Fake monotonic time: only the transport advances it, one second per `recv`.
struct Timeline {
    base: Instant,
    now: Cell<u64>,
    last_read: Cell<u64>,
}

struct FakeClock(Rc<Timeline>);

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let secs = self.0.now.get();
        self.0.last_read.set(secs);
        self.0.base + Duration::from_secs(secs)
    }
}

/// When the peer answers each handshake request: `None` never, `Some(t)` at
/// fake second `t` (or at once if the request is sent later). `turn/start` is
/// answered at once; everything else is never answered.
struct Script {
    initialize: Option<u64>,
    thread_start: Option<u64>,
}

struct TimedPeer {
    time: Rc<Timeline>,
    script: Script,
    /// Responses waiting for their fake second.
    pending: Vec<(u64, Vec<u8>)>,
    /// Every method the loop sent, with the loop's clock reading at the send.
    sent: Vec<(String, u64)>,
}

impl TimedPeer {
    fn schedule(&mut self, at: u64, id: &Value, result: Value) {
        self.pending
            .push((at, line(&json!({ "id": id, "result": result }))));
    }

    fn sent_at(&self, method: &str) -> Option<u64> {
        self.sent.iter().find(|(m, _)| m == method).map(|(_, t)| *t)
    }
}

impl Transport for TimedPeer {
    fn send(&mut self, frame: &Value) -> io::Result<()> {
        let Some(method) = frame.get("method").and_then(Value::as_str) else {
            return Ok(());
        };
        let at = self.time.last_read.get();
        self.sent.push((method.to_string(), at));
        let Some(id) = frame.get("id").cloned() else {
            return Ok(());
        };
        let answer = match method {
            "initialize" => self
                .script
                .initialize
                .map(|t| (t, json!({ "userAgent": "codex" }))),
            "thread/start" => self
                .script
                .thread_start
                .map(|t| (t, json!({ "thread": { "id": testkit::THREAD } }))),
            "turn/start" => Some((at, json!({ "turn": { "id": testkit::TURN } }))),
            _ => None,
        };
        if let Some((when, result)) = answer {
            self.schedule(when.max(at), &id, result);
        }
        Ok(())
    }

    /// Delivers at most one response due at the current fake second, then
    /// advances fake time by one second.
    fn recv(&mut self, _timeout: Duration) -> Result<Option<Value>, FrameError> {
        let now = self.time.now.get();
        if now >= CAP_SECS {
            return Err(FrameError::Eof);
        }
        let due = self.pending.iter().position(|(at, _)| *at <= now);
        let frame = due.map(|index| self.pending.remove(index).1);
        self.time.now.set(now + 1);
        match frame {
            Some(raw) => read_frame(&mut io::Cursor::new(raw), MAX_FRAME_BYTES).map(Some),
            None => Ok(None),
        }
    }
}

/// What one supervised session did.
struct Run {
    outcome: Outcome,
    reason: String,
    /// The loop's last clock reading: the fake second it settled at.
    ended_at: u64,
    peer: TimedPeer,
    thread_started: bool,
}

fn run(script: Script) -> Run {
    let record = invocation();
    let time = Rc::new(Timeline {
        base: Instant::now(),
        now: Cell::new(0),
        last_read: Cell::new(0),
    });
    let clock = FakeClock(Rc::clone(&time));
    let mut peer = TimedPeer {
        time: Rc::clone(&time),
        script,
        pending: Vec::new(),
        sent: Vec::new(),
    };
    let (outcome, reason) = run_session(&record.dir, &mut peer, &clock).expect("run_session");
    Run {
        outcome,
        reason,
        ended_at: time.last_read.get(),
        thread_started: thread_started(&record.dir),
        peer,
    }
}

fn thread_started(dir: &Path) -> bool {
    read_events(dir)
        .expect("events")
        .iter()
        .any(|record| matches!(record.event, NativeEvent::ThreadStarted { .. }))
}

fn assert_startup_timeout(run: &Run, case: &str) {
    assert_eq!(
        (run.outcome, run.reason.as_str()),
        (Outcome::TimedOut, STARTUP_REASON),
        "{case}: settled at {} s",
        run.ended_at
    );
    assert_eq!(
        run.ended_at, STARTUP_DEADLINE_SECS,
        "{case}: live through 119 s, timed out at exactly 120 s from Start"
    );
    assert!(!run.thread_started, "{case}: thread treated as started");
    assert_eq!(run.peer.sent_at("turn/start"), None, "{case}: turn started");
}

/// No answer to `initialize`: live at 119 s, torn down `TimedOut` at 120 s.
fn silent_initialize() {
    let run = run(Script {
        initialize: None,
        thread_start: None,
    });
    assert_startup_timeout(&run, "silent initialize");
    assert_eq!(run.peer.sent_at("initialize"), Some(0));
    assert_eq!(run.peer.sent_at("thread/start"), None);
}

/// `initialize` answered at 100 s, `thread/start` silent: the deadline still
/// counts from Start, so it expires at 120 s, not 220 s.
fn silent_thread_start() {
    let run = run(Script {
        initialize: Some(100),
        thread_start: None,
    });
    assert_eq!(run.peer.sent_at("thread/start"), Some(100));
    assert_startup_timeout(&run, "silent thread/start");
}

/// A handshake response arriving once the deadline passed loses to the timer.
fn late_responses() {
    let late_thread = run(Script {
        initialize: Some(10),
        thread_start: Some(STARTUP_DEADLINE_SECS),
    });
    assert_eq!(late_thread.peer.sent_at("thread/start"), Some(10));
    assert_startup_timeout(&late_thread, "late thread/start");
    let late_init = run(Script {
        initialize: Some(STARTUP_DEADLINE_SECS),
        thread_start: Some(STARTUP_DEADLINE_SECS),
    });
    assert_startup_timeout(&late_init, "late initialize");
    assert_eq!(late_init.peer.sent_at("thread/start"), None);
}

/// Thread started at 60 s: no startup timeout, and the unit deadline counts
/// 1500 s from thread start (interrupt sent at 1560 s, not at 1500 s).
fn on_time_thread_start() {
    let run = run(Script {
        initialize: Some(20),
        thread_start: Some(60),
    });
    assert!(run.thread_started, "thread never started");
    assert_eq!(run.peer.sent_at("turn/start"), Some(60));
    assert_eq!(
        run.peer.sent_at("turn/interrupt"),
        Some(60 + NATIVE_DEADLINE_SECS),
        "unit deadline must arm at thread start"
    );
    assert_eq!(
        (run.outcome, run.reason.as_str()),
        (Outcome::TimedOut, "deadline expired"),
        "settled at {} s",
        run.ended_at
    );
    assert!(run.ended_at > 60 + NATIVE_DEADLINE_SECS);
}

#[test]
#[serial_test::serial]
fn a_worker_whose_thread_never_starts_times_out_at_the_startup_deadline() {
    assert_eq!(STARTUP_DEADLINE_SECS, 120);
    assert_eq!(NATIVE_DEADLINE_SECS, 1500);
    silent_initialize();
    silent_thread_start();
    late_responses();
    on_time_thread_start();
}
