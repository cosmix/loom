use super::*;
use crate::daemon::protocol::CompletionSummary;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn response_reader_retains_split_header_and_body_across_timeouts() {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let mut first = Vec::new();
    write_message(&mut first, &Response::Pong).unwrap();
    let mut second = Vec::new();
    write_message(
        &mut second,
        &Response::OrchestrationComplete {
            summary: CompletionSummary {
                total_duration_secs: 2,
                stages: Vec::new(),
                success_count: 0,
                failure_count: 0,
                plan_path: "next.yaml".to_string(),
            },
        },
    )
    .unwrap();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));

    sender.write_all(&first[..1]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&first[1..3]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&first[3..5]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&first[5..6]).unwrap();
    assert!(reader.poll(&mut receiver).unwrap().is_none());
    sender.write_all(&first[6..]).unwrap();
    assert!(matches!(
        reader.poll(&mut receiver).unwrap(),
        Some(Response::Pong)
    ));
    sender.write_all(&second).unwrap();
    drop(sender);
    match reader.poll(&mut receiver).unwrap() {
        Some(Response::OrchestrationComplete { summary }) => {
            assert_eq!(summary.plan_path, "next.yaml");
        }
        other => panic!("expected subsequent completion, got {other:?}"),
    }
    assert!(reader
        .poll(&mut receiver)
        .unwrap_err()
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::UnexpectedEof));
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
fn response_reader_times_out_an_incomplete_maximum_frame() {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    sender
        .write_all(&(MAX_RESPONSE_BYTES as u32).to_be_bytes())
        .unwrap();
    sender.write_all(b"{").unwrap();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));

    assert!(reader.poll(&mut receiver).unwrap().is_none());
    assert!(reader.started.is_some());
    reader.started = Some(Instant::now() - Duration::from_secs(2));
    assert!(reader
        .poll(&mut receiver)
        .unwrap_err()
        .to_string()
        .contains("exceeded its deadline"));
}

#[test]
fn response_reader_rejects_a_frame_completed_after_its_deadline() {
    struct DelayedBody {
        frame: Vec<u8>,
        offset: usize,
    }

    impl Read for DelayedBody {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.offset == 4 {
                std::thread::sleep(Duration::from_millis(10));
            }
            let count = if self.offset == 0 { 4 } else { buf.len() };
            buf[..count].copy_from_slice(&self.frame[self.offset..self.offset + count]);
            self.offset += count;
            Ok(count)
        }
    }

    let mut frame = Vec::new();
    write_message(&mut frame, &Response::Pong).unwrap();
    let mut stream = DelayedBody { frame, offset: 0 };
    let mut reader = ResponseFrameReader::new(Duration::from_millis(1));

    assert!(reader
        .poll(&mut stream)
        .unwrap_err()
        .to_string()
        .contains("exceeded its deadline"));
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
    match reader.poll(&mut receiver).unwrap() {
        Some(Response::OrchestrationComplete { summary }) => {
            assert_eq!(summary.plan_path, "plan.yaml");
            assert_eq!(summary.total_duration_secs, 1);
        }
        other => panic!("expected completion before EOF, got {other:?}"),
    }
    assert_eq!(
        reader
            .poll(&mut receiver)
            .unwrap_err()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::UnexpectedEof
    );
}
