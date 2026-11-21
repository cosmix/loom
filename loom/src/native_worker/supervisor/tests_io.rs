//! The supervisor's pipe transport and capped provider log.
use std::fs;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use serde_json::json;

use super::io::{CappedLog, PipeTransport};
use crate::native_worker::protocol::frame::{FrameError, Transport};

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
