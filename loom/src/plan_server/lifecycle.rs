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

/// Make an untrusted value safe to embed in a lifecycle line.
pub fn safe(value: &str) -> String {
    crate::context::untrusted::inline_safe(value)
}
