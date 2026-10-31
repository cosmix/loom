use std::io::{self, Read};
use std::time::Duration;

use crate::daemon::ResponseFrameReader;

struct ReadError(io::ErrorKind);

impl Read for ReadError {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from(self.0))
    }
}

#[test]
fn read_timeout_kinds_are_recognized() {
    for kind in [io::ErrorKind::WouldBlock, io::ErrorKind::TimedOut] {
        let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
        assert!(reader.poll(&mut ReadError(kind)).unwrap().is_none());
    }
}

#[test]
fn other_io_errors_are_not_read_timeouts() {
    for kind in [
        io::ErrorKind::UnexpectedEof,
        io::ErrorKind::PermissionDenied,
    ] {
        let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
        assert!(reader.poll(&mut ReadError(kind)).is_err());
    }
}

#[test]
fn a_plain_frame_limit_error_is_not_a_read_timeout() {
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
    let error = reader.poll(&mut &u32::MAX.to_be_bytes()[..]).unwrap_err();
    assert!(error.to_string().contains("exceeds the configured limit"));
}
