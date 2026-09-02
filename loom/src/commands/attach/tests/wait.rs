//! Pins `poll_for_endpoint`'s deadline/early-exit LOGIC with fake
//! `live_sessions`/`probe` closures (no tmux server, no real time longer
//! than a handful of milliseconds), and the honest per-failure-kind
//! diagnosis text.

use super::*;
use crate::orchestrator::terminal::tmux::viewer::tests::stub_session;
use chrono::Utc;
use serial_test::serial;
use std::cell::Cell;
use std::path::Path;
use std::time::Instant;

#[test]
fn poll_for_endpoint_returns_immediately_when_probe_already_succeeds() {
    let session = stub_session("session-a", "stage-a", Utc::now());
    let announced = Cell::new(0u32);

    let outcome = poll_for_endpoint(
        || Ok(vec![session.clone()]),
        |sessions: &[Session]| sessions.first().cloned(),
        Duration::from_secs(30),
        Duration::from_millis(5),
        |_| announced.set(announced.get() + 1),
    )
    .unwrap();

    assert!(matches!(outcome, WaitOutcome::Ready(ref s) if s.id == "session-a"));
    assert_eq!(
        announced.get(),
        0,
        "a probe that succeeds on the first attempt must never announce a wait"
    );
}

#[test]
fn poll_for_endpoint_gives_up_at_the_deadline() {
    let session = stub_session("session-b", "stage-b", Utc::now());
    let deadline = Duration::from_millis(30);
    let started = Instant::now();

    let outcome = poll_for_endpoint(
        || Ok(vec![session.clone()]),
        |_: &[Session]| -> Option<Session> { None },
        deadline,
        Duration::from_millis(5),
        |_| {},
    )
    .unwrap();

    let elapsed = started.elapsed();
    assert!(
        matches!(outcome, WaitOutcome::TimedOut(ref sessions) if sessions.len() == 1),
        "a probe that never succeeds must time out, carrying the last-seen sessions"
    );
    assert!(
        elapsed >= deadline,
        "must not give up before the deadline elapses (waited {elapsed:?})"
    );
    assert!(
        elapsed < deadline * 4,
        "must not burn dramatically more than the deadline (waited {elapsed:?})"
    );
}

#[test]
fn poll_for_endpoint_exits_early_when_the_live_set_empties() {
    let session = stub_session("session-c", "stage-c", Utc::now());
    let reads = Cell::new(0u32);
    // Deliberately much longer than the early exit should ever take, so a
    // regression that falls back to burning the full deadline is caught by
    // the elapsed-time assertion below rather than just making the test slow.
    let deadline = Duration::from_secs(5);
    let started = Instant::now();

    let outcome = poll_for_endpoint(
        || {
            let n = reads.get();
            reads.set(n + 1);
            // First read finds the session live; every read after the first
            // sleep between polls finds it gone.
            if n == 0 {
                Ok(vec![session.clone()])
            } else {
                Ok(vec![])
            }
        },
        |_: &[Session]| -> Option<Session> { None },
        deadline,
        Duration::from_millis(5),
        |_| {},
    )
    .unwrap();

    assert!(matches!(outcome, WaitOutcome::Ended));
    assert!(
        started.elapsed() < deadline,
        "an emptied live set must short-circuit the wait, not burn the full deadline"
    );
}

#[test]
fn poll_for_endpoint_announces_exactly_once_when_it_actually_waits() {
    let session = stub_session("session-d", "stage-d", Utc::now());
    let probe_calls = Cell::new(0u32);
    let announced = Cell::new(0u32);

    let outcome = poll_for_endpoint(
        || Ok(vec![session.clone()]),
        |_: &[Session]| -> Option<Session> {
            let n = probe_calls.get();
            probe_calls.set(n + 1);
            // Ready only on the second probe, so the wait must actually loop
            // (and therefore announce) once before succeeding.
            if n >= 1 {
                Some(session.clone())
            } else {
                None
            }
        },
        Duration::from_secs(5),
        Duration::from_millis(5),
        |count| {
            announced.set(announced.get() + 1);
            assert_eq!(count, 1, "must report the live set size it observed");
        },
    )
    .unwrap();

    assert!(matches!(outcome, WaitOutcome::Ready(_)));
    assert_eq!(
        announced.get(),
        1,
        "must announce exactly once, not per poll"
    );
}

#[test]
fn classify_endpoint_failure_distinguishes_the_three_causes() {
    assert!(matches!(
        classify_endpoint_failure("loom-session-a", "loom-stage-a", false),
        EndpointFailure::SocketMissing
    ));
    assert!(matches!(
        classify_endpoint_failure("loom-session-a", "loom-stage-a", true),
        EndpointFailure::ServerNotAccepting
    ));
    assert!(matches!(
        classify_endpoint_failure("loom session a", "loom-stage-a", true),
        EndpointFailure::UnrenderableIdentifier
    ));
    // An unrenderable identifier must win even when the socket also happens
    // to be missing — checked first because no probe result could ever
    // change that answer.
    assert!(matches!(
        classify_endpoint_failure("loom session a", "loom-stage-a", false),
        EndpointFailure::UnrenderableIdentifier
    ));
}

#[test]
fn describe_failure_keeps_re_run_advice_off_the_permanent_branch_only() {
    let socket_missing = describe_failure(&EndpointFailure::SocketMissing);
    let server_not_accepting = describe_failure(&EndpointFailure::ServerNotAccepting);
    let unrenderable = describe_failure(&EndpointFailure::UnrenderableIdentifier);

    assert!(
        socket_missing.contains("in a moment"),
        "a missing socket is transient and should invite a retry"
    );
    assert!(
        server_not_accepting.contains("in a moment"),
        "a server not yet accepting clients is transient and should invite a retry"
    );
    assert!(
        !unrenderable.contains("in a moment"),
        "an unrenderable identifier is permanent and must never invite a retry: {unrenderable}"
    );
}

#[test]
#[serial]
fn diagnose_sessions_names_the_work_dir_and_every_session() {
    let now = Utc::now();
    let plain = stub_session("session-e", "stage-e", now);
    // A space in the id makes `is_plain_identifier` reject it regardless of
    // whether a real socket happens to exist, so this session's line is
    // deterministically the permanent `UnrenderableIdentifier` branch.
    let hostile = stub_session("session f", "stage f", now + chrono::Duration::seconds(1));
    let sessions = vec![plain.clone(), hostile];
    let work_dir = Path::new("/tmp/some-repo/.loom/work");

    let report = diagnose_sessions(work_dir, &sessions);

    assert!(
        report.contains("/tmp/some-repo/.loom/work"),
        "must name the resolved work dir: {report}"
    );
    assert!(
        report.contains("stage-e"),
        "must name the stage id: {report}"
    );
    assert!(
        report.contains("session-e"),
        "must name the session id: {report}"
    );
    assert!(
        report.contains(&socket_path_for(&socket_name(&plain)).display().to_string()),
        "must name the socket path: {report}"
    );
    assert!(
        report.contains("will not resolve by waiting"),
        "the hostile session's line must carry the permanent-failure wording: {report}"
    );
}
