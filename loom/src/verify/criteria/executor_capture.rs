//! Draining a command's output pipes, and the markers that say why a capture
//! is incomplete. A marker names its stream and cause; [`capture_gaps`] reads
//! the causes back for a failure's text.

use std::io::{ErrorKind, Read};
use std::os::fd::{AsFd, OwnedFd};
use std::process::Child;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Instant;

use crate::process::CleanupDeadline;

use super::super::live_output::LiveSink;
use super::super::result::CriterionResult;

/// Maximum output size for acceptance criteria commands (10MB)
const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024;

pub(in crate::verify::criteria) const OUTPUT_TRUNCATED_MARKER: &str = "[output truncated at 10MB]";
const COLLECTION_TIMED_OUT: &str = "output collection timed out";
const READ_FAILED: &str = "error reading output";

/// Concurrent readers draining a child's piped stdout and stderr.
///
/// IMPORTANT: the pipes must be drained BEFORE waiting for exit. If we wait
/// first, the child may block on write() once the pipe buffer fills up (~64KB
/// on Linux) and never exit, deadlocking the wait.
pub(super) struct OutputReaders {
    stdout: Stream,
    stderr: Stream,
}

impl OutputReaders {
    /// Take both pipes off `child` and start draining them on their own
    /// threads, each also copying what it reads to `live`.
    pub(super) fn spawn(child: &mut Child, live: Option<LiveSink>) -> Self {
        Self {
            stdout: Stream::spawn("stdout", child.stdout.take(), live.clone()),
            stderr: Stream::spawn("stderr", child.stderr.take(), live),
        }
    }

    /// Collect (stdout, stderr) of a leader that exited at `exited`. The reader
    /// threads finish once the killed group has exited and no other holder
    /// keeps a pipe open; one stalled past `deadline` yields a marker rather
    /// than hanging.
    pub(super) fn collect(
        self,
        deadline: &mut CleanupDeadline,
        exited: Instant,
    ) -> (Captured, Captured) {
        let stdout = self.stdout.collect(deadline, exited);
        (stdout, self.stderr.collect(deadline, exited))
    }
}

/// One stream's reader thread.
struct Stream {
    name: &'static str,
    captured: mpsc::Receiver<Captured>,
    /// A duplicate of the pipe's read end, which tells at a timeout whether a
    /// writer still holds the pipe; it is no writer itself.
    probe: Option<OwnedFd>,
}

impl Stream {
    fn spawn<R: Read + AsFd + Send + 'static>(
        name: &'static str,
        stream: Option<R>,
        live: Option<LiveSink>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let probe = stream
            .as_ref()
            .and_then(|stream| stream.as_fd().try_clone_to_owned().ok());
        match stream {
            Some(stream) => {
                thread::spawn(move || {
                    let _ = tx.send(read_stream(name, stream, live.as_ref()));
                });
            }
            None => {
                let _ = tx.send(Captured {
                    text: String::new(),
                    complete: true,
                });
            }
        }
        Self {
            name,
            captured: rx,
            probe,
        }
    }

    fn collect(&self, deadline: &mut CleanupDeadline, exited: Instant) -> Captured {
        let cause = match deadline.recv(&self.captured) {
            Ok(captured) => return captured,
            Err(RecvTimeoutError::Timeout) => {
                let waited = exited.elapsed().as_secs_f64();
                let writer = writer_state(self.probe.as_ref());
                let group = deadline.describe();
                let detail = format!("{} open {waited:.2}s after exit{writer}{group}", self.name);
                marker(COLLECTION_TIMED_OUT, &detail)
            }
            Err(RecvTimeoutError::Disconnected) => {
                let detail = format!("{}: the reader ended without a result", self.name);
                marker(READ_FAILED, &detail)
            }
        };
        Captured {
            text: cause,
            complete: false,
        }
    }
}

/// Whether a writer still holds the pipe `probe` reads, as a clause starting
/// with `, `: a holder the deadline did not wait for, or, with none left, a
/// reader thread that has not yet delivered what it read.
fn writer_state(probe: Option<&OwnedFd>) -> &'static str {
    use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
    let Some(probe) = probe else {
        return "";
    };
    let mut fds = [PollFd::new(probe.as_fd(), PollFlags::POLLIN)];
    if poll(&mut fds, PollTimeout::ZERO).is_err() {
        return "";
    }
    match fds[0].revents() {
        Some(events) if events.contains(PollFlags::POLLHUP) => ", no writer left",
        _ => ", a writer still open",
    }
}

pub(super) struct Captured {
    pub(super) text: String,
    pub(super) complete: bool,
}

fn marker(label: &str, detail: &str) -> String {
    format!("[{label}: {detail}]")
}

/// Why `result`'s capture is incomplete, as its markers say: ` (<causes>)`,
/// or empty when they say nothing.
pub(in crate::verify::criteria) fn capture_cause(result: &CriterionResult) -> String {
    capture_gaps([&result.stdout, &result.stderr])
        .map(|gaps| format!(" ({gaps})"))
        .unwrap_or_default()
}

/// The causes the markers in `streams` give for an incomplete capture, joined
/// by `; `, or `None` when they give none.
fn capture_gaps(streams: [&str; 2]) -> Option<String> {
    let truncated = OUTPUT_TRUNCATED_MARKER.trim_matches(['[', ']']);
    let labels = [COLLECTION_TIMED_OUT, READ_FAILED, truncated];
    let gaps: Vec<&str> = streams
        .into_iter()
        .flat_map(|text| {
            labels.iter().filter_map(move |label| {
                let start = text.rfind(&format!("[{label}"))? + 1;
                let end = start + text[start..].find(']')?;
                Some(&text[start..end])
            })
        })
        .collect();
    (!gaps.is_empty()).then(|| gaps.join("; "))
}

/// Read a stream to string, handling errors gracefully
///
/// Reads output in chunks with a maximum size limit to prevent OOM attacks.
/// If output exceeds MAX_OUTPUT_SIZE, the remaining data is discarded and
/// a truncation message is appended. Every chunk read, kept or not, is also
/// copied to `live`. A read error ends the capture with a marker naming
/// stream `name` and the error.
pub(super) fn read_stream<R: Read>(name: &str, mut stream: R, live: Option<&LiveSink>) -> Captured {
    let mut complete = true;
    let mut truncated = false;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(live) = live {
                    live.write(&chunk[..n]);
                }
                let keep = n.min(MAX_OUTPUT_SIZE.saturating_sub(buf.len()));
                buf.extend_from_slice(&chunk[..keep]);
                truncated |= keep < n;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => {
                buf.extend_from_slice(marker(READ_FAILED, &format!("{name}: {error}")).as_bytes());
                complete = false;
                break;
            }
        }
    }
    if truncated {
        buf.extend_from_slice(b"\n");
        buf.extend_from_slice(OUTPUT_TRUNCATED_MARKER.as_bytes());
    }
    Captured {
        text: String::from_utf8_lossy(&buf).into_owned(),
        complete: complete && !truncated,
    }
}

#[cfg(test)]
#[path = "executor_capture_tests.rs"]
mod tests;
