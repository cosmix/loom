use super::is_read_timeout;

fn wrapped(kind: std::io::ErrorKind) -> anyhow::Error {
    anyhow::Error::new(std::io::Error::new(kind, "test error"))
        .context("Failed to read message length")
}

#[test]
fn read_timeout_kinds_are_recognized() {
    assert!(is_read_timeout(&wrapped(std::io::ErrorKind::WouldBlock)));
    assert!(is_read_timeout(&wrapped(std::io::ErrorKind::TimedOut)));
}

#[test]
fn other_io_errors_are_not_read_timeouts() {
    assert!(!is_read_timeout(&wrapped(
        std::io::ErrorKind::UnexpectedEof
    )));
    assert!(!is_read_timeout(&wrapped(
        std::io::ErrorKind::PermissionDenied
    )));
}

#[test]
fn a_plain_frame_limit_error_is_not_a_read_timeout() {
    // What `read_frame_length` (wire.rs) produces on the garbage length
    // prefix a misaligned stream reads next - no io::Error in the chain
    // at all, so it must fall through to the reconnect path rather than
    // being mistaken for an idle timeout.
    let error = anyhow::anyhow!("Message frame exceeds the configured limit");
    assert!(!is_read_timeout(&error));
}
