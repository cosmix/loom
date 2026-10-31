use super::*;
use crate::daemon::protocol::CompletionSummary;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[test]
fn response_reader_retains_split_header_and_body_across_timeouts() {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let mut first = Vec::new();
    write_message(&mut first, &Response::Pong).unwrap();
    let mut second = Vec::new();
    write_message(&mut second, &Response::Ok).unwrap();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));

    sender.write_all(&first[..2]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&first[2..6]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&first[6..]).unwrap();
    assert!(matches!(
        reader.poll(&mut receiver).unwrap(),
        Some(Response::Pong)
    ));
    sender.write_all(&second).unwrap();
    assert!(matches!(
        reader.poll(&mut receiver).unwrap(),
        Some(Response::Ok)
    ));
}

#[test]
fn response_reader_bounds_partial_frames() {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    sender.write_all(&[0, 0]).unwrap();
    let mut reader = ResponseFrameReader::new(Duration::ZERO);
    assert!(reader.poll(&mut receiver).is_err());

    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    sender
        .write_all(&((MAX_RESPONSE_BYTES + 1) as u32).to_be_bytes())
        .unwrap();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
    assert!(reader
        .poll(&mut receiver)
        .unwrap_err()
        .to_string()
        .contains("configured limit"));
}

#[test]
fn split_completion_is_delivered_before_eof() {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let response = Response::OrchestrationComplete {
        summary: CompletionSummary {
            total_duration_secs: 1,
            stages: Vec::new(),
            success_count: 0,
            failure_count: 0,
            plan_path: "plan.yaml".to_string(),
        },
    };
    let mut frame = Vec::new();
    write_message(&mut frame, &response).unwrap();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));

    sender.write_all(&frame[..3]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&frame[3..10]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&frame[10..]).unwrap();
    drop(sender);
    assert!(matches!(
        reader.poll(&mut receiver).unwrap(),
        Some(Response::OrchestrationComplete { .. })
    ));
    assert!(reader.poll(&mut receiver).is_err());
}
