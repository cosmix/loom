//! Tests for the subscriber-retention rules of the status broadcaster.
//!
//! These guard the daemon's own-payload failure mode: `write_json_frame`
//! refuses an oversized frame with the same `Err` a dead peer returns, so
//! retaining subscribers on `write_message(..).is_ok()` alone evicted every
//! dashboard over a bug on the daemon's side. Reverting
//! `broadcast_retaining_live` to that one-liner must fail these.

use super::*;
use crate::daemon::protocol::read_message;
use std::io::Read as _;

/// A response whose serialized form is comfortably past the frame limit.
fn oversized_response() -> Response {
    Response::LogLine {
        line: "x".repeat(MAX_RESPONSE_BYTES + 1),
    }
}

fn small_response() -> Response {
    Response::LogLine {
        line: "build finished".to_string(),
    }
}

/// Two connected pairs; the writer ends become the subscriber list and the
/// reader ends are kept alive by the caller.
fn two_subscribers() -> (Vec<UnixStream>, Vec<UnixStream>) {
    let (writer_a, reader_a) = UnixStream::pair().unwrap();
    let (writer_b, reader_b) = UnixStream::pair().unwrap();
    (vec![writer_a, writer_b], vec![reader_a, reader_b])
}

#[test]
fn an_oversized_response_evicts_nobody() {
    let (mut subscribers, _readers) = two_subscribers();
    let mut oversized_logged = false;

    broadcast_retaining_live(
        &mut subscribers,
        &oversized_response(),
        &mut oversized_logged,
    );

    assert_eq!(subscribers.len(), 2);
    assert!(oversized_logged);
}

#[test]
fn an_oversized_response_is_reported_to_subscribers_as_an_error() {
    let (mut subscribers, mut readers) = two_subscribers();
    let mut oversized_logged = false;

    broadcast_retaining_live(
        &mut subscribers,
        &oversized_response(),
        &mut oversized_logged,
    );

    for reader in &mut readers {
        let response: Response = read_message(reader).unwrap();
        let Response::Error { message } = response else {
            panic!("expected an Error response, got {response:?}");
        };
        assert!(
            message.contains(&MAX_RESPONSE_BYTES.to_string()),
            "{message}"
        );
    }
}

#[test]
fn a_persistent_overflow_is_logged_once_and_clears_when_it_ends() {
    let (mut subscribers, _readers) = two_subscribers();
    let mut oversized_logged = false;

    for _ in 0..3 {
        broadcast_retaining_live(
            &mut subscribers,
            &oversized_response(),
            &mut oversized_logged,
        );
        assert!(oversized_logged);
    }

    broadcast_retaining_live(&mut subscribers, &small_response(), &mut oversized_logged);

    assert!(!oversized_logged);
    assert_eq!(subscribers.len(), 2);
}

#[test]
fn a_dead_peer_is_evicted_while_a_live_one_is_kept() {
    let (live_writer, mut live_reader) = UnixStream::pair().unwrap();
    let (dead_writer, dead_reader) = UnixStream::pair().unwrap();
    drop(dead_reader);
    let mut subscribers = vec![live_writer, dead_writer];
    let mut oversized_logged = false;

    broadcast_retaining_live(&mut subscribers, &small_response(), &mut oversized_logged);

    assert_eq!(subscribers.len(), 1);
    let response: Response = read_message(&mut live_reader).unwrap();
    assert!(matches!(response, Response::LogLine { .. }));
}

#[test]
fn a_dead_peer_is_evicted_even_while_the_payload_is_oversized() {
    let (live_writer, _live_reader) = UnixStream::pair().unwrap();
    let (dead_writer, dead_reader) = UnixStream::pair().unwrap();
    drop(dead_reader);
    let mut subscribers = vec![live_writer, dead_writer];
    let mut oversized_logged = false;

    broadcast_retaining_live(
        &mut subscribers,
        &oversized_response(),
        &mut oversized_logged,
    );

    assert_eq!(subscribers.len(), 1);
}

#[test]
fn frame_overflow_flags_only_the_oversized_response() {
    assert!(frame_overflow(&small_response()).is_none());

    let notice = frame_overflow(&oversized_response()).expect("oversized response");
    assert!(notice.contains(&MAX_RESPONSE_BYTES.to_string()), "{notice}");
}

#[test]
fn the_overflow_notice_itself_fits_the_frame() {
    let notice = frame_overflow(&oversized_response()).expect("oversized response");
    let error = Response::Error { message: notice };

    assert!(frame_overflow(&error).is_none());
}

#[test]
fn a_subscriber_that_reads_nothing_still_receives_the_next_tick() {
    // The writer end must not be left in a half-written state by the skipped
    // tick: an oversized payload is never written at all, so the following
    // ordinary response is the first thing on the wire.
    let (writer, mut reader) = UnixStream::pair().unwrap();
    let mut subscribers = vec![writer];
    let mut oversized_logged = false;

    broadcast_retaining_live(
        &mut subscribers,
        &oversized_response(),
        &mut oversized_logged,
    );
    broadcast_retaining_live(&mut subscribers, &small_response(), &mut oversized_logged);

    let first: Response = read_message(&mut reader).unwrap();
    assert!(matches!(first, Response::Error { .. }));
    let second: Response = read_message(&mut reader).unwrap();
    let Response::LogLine { line } = second else {
        panic!("expected a LogLine response, got {second:?}");
    };
    assert_eq!(line, "build finished");

    // Nothing else was queued behind them.
    reader.set_nonblocking(true).unwrap();
    let mut trailing = [0u8; 1];
    assert!(reader.read(&mut trailing).is_err());
}
