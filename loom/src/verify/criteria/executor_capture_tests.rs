use super::*;
use std::io::Cursor;
use std::time::Duration;

fn read_stream_to_string<R: Read>(stream: R) -> String {
    read_stream("stdout", stream, None).text
}

/// Fails each read with the next of `errors` once `data` is read.
struct Failing {
    data: &'static [u8],
    errors: Vec<std::io::Error>,
}

impl Read for Failing {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if let Some((&byte, rest)) = self.data.split_first() {
            self.data = rest;
            bytes[0] = byte;
            return Ok(1);
        }
        match self.errors.pop() {
            Some(error) => Err(error),
            None => Ok(0),
        }
    }
}

#[test]
fn test_read_stream_small_input() {
    let data = b"hello world";
    let result = read_stream_to_string(Cursor::new(data));
    assert_eq!(result, "hello world");
}

#[test]
fn test_read_stream_empty_input() {
    let data: &[u8] = b"";
    let result = read_stream_to_string(Cursor::new(data));
    assert_eq!(result, "");
}

#[test]
fn test_read_stream_truncates_at_limit() {
    let data = vec![b'x'; MAX_OUTPUT_SIZE + 1000];
    let result = read_stream_to_string(Cursor::new(data));
    assert!(result.contains(OUTPUT_TRUNCATED_MARKER));
    assert!(result.len() <= MAX_OUTPUT_SIZE + 50);
    assert_eq!(
        capture_gaps([&result, ""]).as_deref(),
        Some("output truncated at 10MB")
    );
}

#[test]
fn test_read_stream_exact_limit() {
    let data = vec![b'y'; MAX_OUTPUT_SIZE];
    let result = read_stream_to_string(Cursor::new(data));
    assert!(!result.contains(OUTPUT_TRUNCATED_MARKER));
    assert_eq!(result.len(), MAX_OUTPUT_SIZE);
}

#[test]
fn test_max_output_size_is_10mb() {
    assert_eq!(MAX_OUTPUT_SIZE, 10 * 1024 * 1024);
}

#[test]
fn a_read_error_names_its_stream_and_errno() {
    let errors = vec![std::io::Error::from_raw_os_error(libc::EIO)];
    let capture = read_stream("stderr", Failing { data: b"x", errors }, None);
    assert!(!capture.complete);
    assert!(capture.text.starts_with("x[error reading output: stderr: "));
    let gaps = capture_gaps(["", &capture.text]).unwrap();
    assert!(gaps.starts_with("error reading output: stderr: "), "{gaps}");
    assert!(
        gaps.ends_with(&format!("(os error {})", libc::EIO)),
        "{gaps}"
    );
}

#[test]
fn an_interrupted_read_is_retried() {
    let errors = vec![std::io::Error::from(ErrorKind::Interrupted)];
    let capture = read_stream("stdout", Failing { data: b"x", errors }, None);
    assert!(capture.complete, "{}", capture.text);
    assert_eq!(capture.text, "x");
}

#[test]
fn a_pipe_a_writer_still_holds_times_out_naming_stream_wait_and_writer() {
    let (reader, writer) = std::io::pipe().unwrap();
    let stream = Stream::spawn("stdout", Some(reader), None);
    let exited = Instant::now();
    let mut deadline = CleanupDeadline::fixed(exited + Duration::from_millis(100));
    let capture = stream.collect(&mut deadline, exited);
    drop(writer);
    assert!(!capture.complete);
    let gaps = capture_gaps([&capture.text, ""]).unwrap();
    assert!(
        gaps.starts_with("output collection timed out: stdout open "),
        "{gaps}"
    );
    assert!(
        gaps.ends_with("s after exit, a writer still open"),
        "{gaps}"
    );
}

#[test]
fn a_closed_pipe_whose_reader_has_not_answered_names_no_writer() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(writer);
    let (_sender, captured) = mpsc::channel();
    let stream = Stream {
        name: "stderr",
        captured,
        probe: Some(OwnedFd::from(reader)),
    };
    let exited = Instant::now();
    let mut deadline = CleanupDeadline::fixed(exited);
    let capture = stream.collect(&mut deadline, exited);
    let gaps = capture_gaps(["", &capture.text]).unwrap();
    assert!(
        gaps.starts_with("output collection timed out: stderr open "),
        "{gaps}"
    );
    assert!(gaps.ends_with(", no writer left"), "{gaps}");
}

#[test]
fn a_reader_that_ends_without_a_result_is_a_read_error() {
    let (sender, captured) = mpsc::channel();
    drop(sender);
    let stream = Stream {
        name: "stdout",
        captured,
        probe: None,
    };
    let exited = Instant::now();
    let capture = stream.collect(&mut CleanupDeadline::fixed(exited), exited);
    assert!(!capture.complete);
    assert_eq!(
        capture_gaps([&capture.text, ""]).as_deref(),
        Some("error reading output: stdout: the reader ended without a result")
    );
}

#[test]
fn complete_output_gives_no_gap() {
    assert_eq!(capture_gaps(["ok\n", ""]), None);
}
