//! Tests for lifecycle lines.

use std::io::{self, Write};

use super::lifecycle::{safe, write_lines};

#[test]
fn safe_preserves_ordinary_text() {
    assert_eq!(safe("project/example"), "project/example");
}

#[test]
fn safe_flattens_untrusted_line_controls() {
    assert_eq!(
        safe("first\n\x1b[31m\u{202e}second `name`"),
        "first [31m second ˋnameˋ"
    );
}

#[test]
fn safe_bounds_long_values() {
    let value = "a".repeat(201);
    assert_eq!(safe(&value), format!("{}…", "a".repeat(199)));
}

/// A reader that went away: every write and flush fails with a broken pipe.
struct ClosedPipe;

impl Write for ClosedPipe {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
}

#[test]
fn startup_lines_to_a_closed_pipe_do_not_panic() {
    write_lines(&mut ClosedPipe, ["read-only: a", "read-only: b"]);
}

#[test]
fn startup_lines_are_written_one_per_line() {
    let mut out = Vec::new();
    write_lines(&mut out, ["first", "second"]);
    assert_eq!(String::from_utf8(out).unwrap(), "first\nsecond\n");
}
