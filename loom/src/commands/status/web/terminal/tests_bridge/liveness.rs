//! A bridge must notice a peer that is connected but no longer taking part:
//! one that never reads, one that goes silent, and one that merely reads
//! slowly (which it must keep).
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::{Message, WebSocket};

use super::super::bridge::Timing;
use super::super::protocol::Mode;
use super::{
    is_transient, join_within, skip_bridge_test, start_bridge, start_bridge_with, wait_for_binary,
};

/// Short keepalive and silence deadlines, so a silent peer is dropped in about
/// a second instead of a minute.
fn quick_silence() -> Timing {
    Timing {
        keepalive: Duration::from_millis(200),
        silence: Duration::from_secs(1),
        ..Timing::DEFAULT
    }
}

#[test]
fn bridge_disconnects_a_peer_that_never_reads() {
    if skip_bridge_test("bridge_disconnects_a_peer_that_never_reads") {
        return;
    }
    // `yes` never stops printing and the client never reads, so with 8 KiB
    // kernel buffers the bridge's flush stalls at once and never clears.
    let timing = Timing {
        stall: Duration::from_secs(1),
        ..Timing::DEFAULT
    };
    let fixture = start_bridge_with("yes", Mode::Control, Some(8 * 1024), timing);
    // The client socket stays open until the end of the test: a closed one
    // would end the bridge for a different reason.
    let _client = fixture.socket;
    join_within(fixture.server, Duration::from_secs(10));
}

/// Output a slow reader takes per tick.
const TICK_BYTES: usize = 16 * 1024;

fn saw_end(seen: &[u8]) -> bool {
    seen.windows(3).any(|part| part == b"END")
}

/// One tick of a slow reader: add `TICK_BYTES` to `budget`, then take messages
/// while it is positive, the socket has more, and END is not in. A message
/// can be as large as a tick, so the overshoot is carried into the next tick
/// as debt; without it a tick could take nearly two ticks' worth and the
/// transfer finish too fast. Stopping at END keeps the read from reaching
/// the bridge's close, which follows it at once.
fn take_tick(socket: &mut WebSocket<TcpStream>, seen: &mut Vec<u8>, budget: &mut isize) {
    *budget += TICK_BYTES as isize;
    while *budget > 0 && !saw_end(seen) {
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                *budget -= bytes.len() as isize;
                seen.extend_from_slice(&bytes);
            }
            Ok(Message::Close(frame)) => panic!("the bridge dropped a slow reader: {frame:?}"),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error)) if is_transient(&error) => return,
            Err(error) => panic!("read bridge output: {error}"),
        }
    }
}

#[test]
fn bridge_survives_a_client_that_reads_slowly() {
    if skip_bridge_test("bridge_survives_a_client_that_reads_slowly") {
        return;
    }
    let timing = Timing {
        stall: Duration::from_millis(1500),
        ..Timing::DEFAULT
    };
    let mut fixture = start_bridge_with(
        "head -c 307200 /dev/zero | tr '\\0' 'x'; printf 'END\\n'",
        Mode::Control,
        Some(8 * 1024),
        timing,
    );
    // 16 KiB every 250 ms: 300 KiB is about 19 ticks, so the transfer takes
    // more than twice the stall time while the client keeps draining. The pace
    // is set in bytes, not messages: a PTY read returns 1 KiB on macOS and up
    // to 16 KiB on Linux, so one message per tick took 300 ticks (75 s) there.
    let started = Instant::now();
    let mut seen = Vec::new();
    let mut budget = 0;
    while !saw_end(&seen) {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "no END in 60 s"
        );
        take_tick(&mut fixture.socket, &mut seen, &mut budget);
        thread::sleep(Duration::from_millis(250));
    }
    assert!(
        started.elapsed() >= Duration::from_secs(4),
        "the transfer ended in {:?}, too fast to show the bridge tolerating a slow reader",
        started.elapsed()
    );
    // The child exits right after END, so the bridge may already have closed
    // the connection; a failed client close is not a test failure.
    let _ = fixture.socket.close(None);
    join_within(fixture.server, Duration::from_secs(5));
}

#[test]
fn bridge_closes_a_silent_peer() {
    if skip_bridge_test("bridge_closes_a_silent_peer") {
        return;
    }
    let fixture = start_bridge_with("sleep 30", Mode::Control, None, quick_silence());
    // Never read: tungstenite answers a ping only from `read`, so this peer
    // says nothing at all.
    let _client = fixture.socket;
    join_within(fixture.server, Duration::from_secs(8));
}

#[test]
fn bridge_keeps_a_peer_that_answers_pings() {
    if skip_bridge_test("bridge_keeps_a_peer_that_answers_pings") {
        return;
    }
    let mut fixture = start_bridge_with("sleep 30", Mode::Control, None, quick_silence());
    // Three seconds is three times the silence deadline. Each `read` answers
    // the pings that arrived since the last one.
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        match fixture.socket.read() {
            Ok(Message::Close(frame)) => panic!("the bridge dropped a live peer: {frame:?}"),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error)) if is_transient(&error) => {}
            Err(error) => panic!("read bridge output: {error}"),
        }
    }
    assert!(
        !fixture.server.is_finished(),
        "the bridge dropped a live peer"
    );
    fixture.socket.close(None).expect("close bridge client");
    join_within(fixture.server, Duration::from_secs(5));
}

#[test]
fn a_viewer_resize_reaches_the_pty() {
    if skip_bridge_test("a_viewer_resize_reaches_the_pty") {
        return;
    }
    let script = "sleep 1; stty size; sleep 30";
    let resize = r#"{"resize":{"cols":100,"rows":30}}"#;
    let mut viewer = start_bridge(script, Mode::View);
    viewer
        .socket
        .send(Message::text(resize))
        .expect("send viewer resize");
    let viewer_resized = wait_for_binary(&mut viewer.socket, b"30 100", Duration::from_secs(10));
    // Positive control: both modes resize the PTY, so a controller reaching
    // 30 by 100 shows the frame is well formed and the viewer's result is
    // the bridge's doing.
    let mut controller = start_bridge(script, Mode::Control);
    controller
        .socket
        .send(Message::text(resize))
        .expect("send controller resize");
    let resized = wait_for_binary(&mut controller.socket, b"30 100", Duration::from_secs(10));
    viewer.socket.close(None).expect("close viewer");
    controller.socket.close(None).expect("close controller");
    join_within(viewer.server, Duration::from_secs(5));
    join_within(controller.server, Duration::from_secs(5));
    assert!(
        viewer_resized,
        "the viewer's resize did not reach the PTY (expected 30 rows by 100 columns)"
    );
    assert!(
        resized,
        "the controller's PTY did not resize to 30 rows by 100 columns"
    );
}
