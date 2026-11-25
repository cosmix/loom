//! Lifecycle terminal lines.

use std::io::{self, Write};

/// Print a timestamped lifecycle message and flush it immediately.
pub fn line(message: &str) {
    let mut stdout = io::stdout().lock();
    // A dead terminal (SIGHUP, closed pipe) must not panic the server: the
    // line is informational, so write and flush errors are ignored.
    let _ = writeln!(
        stdout,
        "{} {}",
        chrono::Local::now().format("%H:%M:%S"),
        message
    );
    let _ = stdout.flush();
}

/// Print startup lines to stdout. `println!` panics when the reader has gone away (`loom plan |
/// head`, a closed terminal), which would fail the project load, so errors are ignored.
pub fn print_lines<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) {
    let mut stdout = io::stdout().lock();
    write_lines(&mut stdout, lines);
}

pub(super) fn write_lines<S: AsRef<str>>(out: &mut impl Write, lines: impl IntoIterator<Item = S>) {
    for line in lines {
        let _ = writeln!(out, "{}", line.as_ref());
    }
    let _ = out.flush();
}

/// Make an untrusted value safe to embed in a lifecycle line.
pub fn safe(value: &str) -> String {
    crate::context::untrusted::inline_safe(value)
}
