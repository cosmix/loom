//! Tests for the subscriber-retention rules of the status broadcaster.
//!
//! These guard the daemon's own-payload failure mode: `write_json_frame`
//! refuses an oversized frame with the same `Err` a dead peer returns, so
//! retaining subscribers on `write_message(..).is_ok()` alone evicted every
//! dashboard over a bug on the daemon's side. Reverting
//! `broadcast_retaining_live` to that one-liner must fail these.

use super::*;
use crate::daemon::protocol::read_message;
use crate::daemon::wire::{response_serialization_count, MAX_RESPONSE_BYTES};
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
fn two_subscribers() -> (Vec<Subscriber>, Vec<UnixStream>) {
    let (writer_a, reader_a) = UnixStream::pair().unwrap();
    let (writer_b, reader_b) = UnixStream::pair().unwrap();
    (
        vec![
            Arc::new(Mutex::new(writer_a)),
            Arc::new(Mutex::new(writer_b)),
        ],
        vec![reader_a, reader_b],
    )
}

fn counted_broadcast(
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
    response: Response,
    attempted: std::sync::mpsc::Sender<()>,
) -> std::thread::JoinHandle<usize> {
    std::thread::spawn(move || {
        let before = response_serialization_count();
        broadcast_to_subscribers_with_hook(&subscribers, &response, &mut false, &mut || {
            attempted.send(()).unwrap()
        });
        response_serialization_count() - before
    })
}

#[test]
fn simultaneous_broadcasts_encode_once_each_and_keep_both_streams_framed() {
    let (writers, mut readers) = two_subscribers();
    for reader in &mut readers {
        reader
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
    }
    let blocked_writer = Arc::clone(&writers[0]);
    let held = blocked_writer.lock().unwrap();
    let subscribers = Arc::new(Mutex::new(writers));
    let (attempted, ready) = std::sync::mpsc::channel();
    let first = counted_broadcast(Arc::clone(&subscribers), Response::Pong, attempted.clone());
    let second = counted_broadcast(
        Arc::clone(&subscribers),
        Response::LogLine { line: "two".into() },
        attempted,
    );
    for _ in 0..2 {
        ready
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
    }
    drop(held);
    assert_eq!(first.join().unwrap(), 1);
    assert_eq!(second.join().unwrap(), 1);

    for reader in &mut readers {
        let first: Response = read_message(&mut *reader).unwrap();
        let second: Response = read_message(&mut *reader).unwrap();
        assert!(matches!(
            (&first, &second),
            (Response::Pong, Response::LogLine { line })
                | (Response::LogLine { line }, Response::Pong) if line == "two"
        ));
    }
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
    // A bare `drop` only releases this thread's reference to the reader fd;
    // another test thread can `fork` (via `std::process::Command`) at any
    // moment and inherit a duplicate that keeps the socket alive until that
    // child reaches its own `exec`, making eviction flaky. `shutdown` marks
    // the socket itself as closed, so the write below fails deterministically
    // no matter how many forked copies of the fd exist.
    dead_reader.shutdown(std::net::Shutdown::Both).unwrap();
    drop(dead_reader);
    let mut subscribers = vec![
        Arc::new(Mutex::new(live_writer)),
        Arc::new(Mutex::new(dead_writer)),
    ];
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
    // See the comment in `a_dead_peer_is_evicted_while_a_live_one_is_kept`: a
    // concurrent `fork` elsewhere can hold a duplicate of this fd, so a bare
    // `drop` alone would not make the write below fail deterministically.
    dead_reader.shutdown(std::net::Shutdown::Both).unwrap();
    drop(dead_reader);
    let mut subscribers = vec![
        Arc::new(Mutex::new(live_writer)),
        Arc::new(Mutex::new(dead_writer)),
    ];
    let mut oversized_logged = false;

    broadcast_retaining_live(
        &mut subscribers,
        &oversized_response(),
        &mut oversized_logged,
    );

    assert_eq!(subscribers.len(), 1);
}

#[test]
fn a_slow_reader_is_evicted_within_the_frame_deadline() {
    let (writer, _reader) = UnixStream::pair().unwrap();
    let mut subscribers = vec![Arc::new(Mutex::new(writer))];
    let response = Response::LogLine {
        line: "x".repeat(MAX_RESPONSE_BYTES / 2),
    };

    let started = std::time::Instant::now();
    broadcast_retaining_live(&mut subscribers, &response, &mut false);

    assert!(subscribers.is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(6));
}

#[cfg(target_os = "linux")]
#[test]
fn a_trickle_reader_cannot_extend_the_whole_frame_deadline() {
    use std::os::fd::AsRawFd;
    use std::sync::atomic::{AtomicBool, Ordering};
    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    let buffer_size: libc::c_int = 8192;
    assert_eq!(
        // SAFETY: writer owns a valid socket descriptor; buffer_size remains live and
        // points to an initialized c_int for the duration of this call.
        unsafe {
            libc::setsockopt(
                writer.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                (&raw const buffer_size).cast(),
                std::mem::size_of_val(&buffer_size) as libc::socklen_t,
            )
        },
        0
    );
    let done = Arc::new(AtomicBool::new(false));
    let draining = {
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let mut buffer = [0; 4096];
            while !done.load(Ordering::Relaxed) && reader.read(&mut buffer).unwrap_or(0) != 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        })
    };

    let started = std::time::Instant::now();
    let error = super::super::client::write_bounded_frame_with_timeout(
        &mut writer,
        &vec![b'x'; 1024 * 1024],
        std::time::Duration::from_millis(120),
    )
    .unwrap_err();
    done.store(true, Ordering::Relaxed);
    let _ = writer.shutdown(std::net::Shutdown::Both);
    draining.join().unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < std::time::Duration::from_millis(300));
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
    let mut subscribers = vec![Arc::new(Mutex::new(writer))];
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

#[test]
fn a_failed_collection_becomes_an_error_notice_logged_once_per_entry() {
    let mut logged = false;

    let first = status_or_failure_notice(Err(anyhow::anyhow!("bad stage file")), &mut logged);
    assert!(logged);
    match first {
        Response::Error { message } => {
            assert_eq!(message, "Status collection failed: bad stage file");
        }
        other => panic!("expected an error notice, got {other:?}"),
    }

    // Still failing: the notice is sent again, the latch stays set.
    let again = status_or_failure_notice(Err(anyhow::anyhow!("bad stage file")), &mut logged);
    assert!(matches!(again, Response::Error { .. }));
    assert!(logged);

    // Recovery passes the response through and re-arms the latch.
    let recovered = status_or_failure_notice(Ok(small_response()), &mut logged);
    assert!(matches!(recovered, Response::LogLine { .. }));
    assert!(!logged);
}
