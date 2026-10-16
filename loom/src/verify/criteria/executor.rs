//! Low-level command execution for acceptance criteria

use crate::process::OwnedProcess;
use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::Child;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::models::stage::CommandConfinement;

use super::config::DEFAULT_COMMAND_TIMEOUT;
use super::confine::{self, CommandSpec, PreparedCommand};
use super::result::CriterionResult;

/// Maximum output size for acceptance criteria commands (10MB)
const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024;

pub(super) const OUTPUT_TRUNCATED_MARKER: &str = "[output truncated at 10MB]";
pub(super) const OUTPUT_COLLECTION_TIMEOUT_MARKER: &str = "[output collection timed out]";
pub(super) const OUTPUT_READ_ERROR_MARKER: &str = "[error reading output]";

/// Run a single acceptance criterion (shell command) with default timeout
///
/// This is a convenience wrapper around `run_single_criterion_with_timeout` that uses
/// the default timeout setting.
pub fn run_single_criterion(command: &str, working_dir: Option<&Path>) -> Result<CriterionResult> {
    run_single_criterion_with_timeout(command, working_dir, DEFAULT_COMMAND_TIMEOUT)
}

/// Run a single acceptance criterion (shell command) with specified timeout
///
/// Executes the command using the system shell and captures all output.
/// Returns a CriterionResult with execution details.
///
/// If `working_dir` is provided, the command will be executed in that directory.
///
/// The command will be terminated if it exceeds the specified `timeout` duration.
/// When this happens, the result will have `timed_out` set to true and `success`
/// set to false.
///
/// The command runs at the default confinement level
/// ([`CommandConfinement::Confined`]); callers that know the stage's resolved
/// level use [`run_spec_with_timeout`] instead.
pub fn run_single_criterion_with_timeout(
    command: &str,
    working_dir: Option<&Path>,
    timeout: Duration,
) -> Result<CriterionResult> {
    run_spec_with_timeout(
        &CommandSpec::shell(command),
        working_dir,
        timeout,
        CommandConfinement::default(),
    )
}

/// Run one command spec with a timeout and a confinement level.
///
/// This is the single implementation of the run loop: spawn, drain both pipes
/// concurrently, wait with a timeout, and kill the whole process group if the
/// deadline passes. Everything else in this module funnels into it.
pub fn run_spec_with_timeout(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    timeout: Duration,
    confinement: CommandConfinement,
) -> Result<CriterionResult> {
    let prepared = confine::prepare_confined(spec, working_dir, confinement)?;
    run_prepared_with_timeout(prepared, timeout)
}

/// Execute a command already prepared by the confinement path.
pub(super) fn run_prepared_with_timeout(
    mut prepared: PreparedCommand,
    timeout: Duration,
) -> Result<CriterionResult> {
    let start = Instant::now();
    let display = prepared.display().to_string();
    let mut child = OwnedProcess::new(prepared.spawn_retaining_resources()?);
    let output = OutputReaders::spawn(child.child());
    let wait_result = child
        .wait(timeout)
        .with_context(|| format!("Failed to wait for command: {display}"))?;
    let cleanup_deadline = child.retire();
    let (stdout, mut stderr) = output.collect(cleanup_deadline);
    let complete = stdout.complete && stderr.complete;
    if wait_result.is_none() {
        stderr
            .text
            .push_str(&format!("\n[command timeout after {timeout:?}]"));
    }
    let mut result = CriterionResult::new(
        display,
        wait_result.is_some_and(|status| status.success()) && complete,
        stdout.text,
        stderr.text,
        wait_result.and_then(|status| status.code()),
        start.elapsed(),
        wait_result.is_none(),
    );
    result.output_complete = complete;
    Ok(result)
}

/// Concurrent readers draining a child's piped stdout and stderr.
///
/// IMPORTANT: the pipes must be drained BEFORE waiting for exit. If we wait
/// first, the child may block on write() once the pipe buffer fills up (~64KB
/// on Linux) and never exit, deadlocking the wait.
struct OutputReaders {
    stdout: mpsc::Receiver<Captured>,
    stderr: mpsc::Receiver<Captured>,
}

impl OutputReaders {
    /// Take both pipes off `child` and start draining them on their own threads.
    fn spawn(child: &mut Child) -> Self {
        Self {
            stdout: spawn_reader(child.stdout.take()),
            stderr: spawn_reader(child.stderr.take()),
        }
    }

    /// Collect (stdout, stderr). The reader threads should finish promptly once
    /// the process exits; a stalled one yields a marker rather than hanging.
    fn collect(self, deadline: Instant) -> (Captured, Captured) {
        (
            collect_stream(self.stdout, deadline),
            collect_stream(self.stderr, deadline),
        )
    }
}

fn spawn_reader<R: Read + Send + 'static>(stream: Option<R>) -> mpsc::Receiver<Captured> {
    let (tx, rx) = mpsc::channel();
    match stream {
        Some(stream) => {
            thread::spawn(move || {
                let _ = tx.send(read_stream(stream));
            });
        }
        None => {
            let _ = tx.send(Captured {
                text: String::new(),
                complete: true,
            });
        }
    }
    rx
}

fn collect_stream(stream: mpsc::Receiver<Captured>, deadline: Instant) -> Captured {
    stream
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap_or_else(|_| Captured {
            text: OUTPUT_COLLECTION_TIMEOUT_MARKER.to_string(),
            complete: false,
        })
}

/// Read a stream to string, handling errors gracefully
///
/// Reads output in chunks with a maximum size limit to prevent OOM attacks.
/// If output exceeds MAX_OUTPUT_SIZE, the remaining data is discarded and
/// a truncation message is appended.
struct Captured {
    text: String,
    complete: bool,
}

fn read_stream<R: Read>(mut stream: R) -> Captured {
    let mut complete = true;
    let mut truncated = false;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let keep = n.min(MAX_OUTPUT_SIZE.saturating_sub(buf.len()));
                buf.extend_from_slice(&chunk[..keep]);
                truncated |= keep < n;
            }
            Err(_) => {
                buf.extend_from_slice(OUTPUT_READ_ERROR_MARKER.as_bytes());
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
fn read_stream_to_string<R: Read>(stream: R) -> String {
    read_stream(stream).text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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
        // Create data larger than MAX_OUTPUT_SIZE
        let data = vec![b'x'; MAX_OUTPUT_SIZE + 1000];
        let result = read_stream_to_string(Cursor::new(data));

        // Should contain the truncation message
        assert!(result.contains(OUTPUT_TRUNCATED_MARKER));

        // Should not exceed MAX_OUTPUT_SIZE + truncation message length
        assert!(result.len() <= MAX_OUTPUT_SIZE + 50);
    }

    #[test]
    fn test_read_stream_exact_limit() {
        // Data exactly at the limit should NOT be truncated
        let data = vec![b'y'; MAX_OUTPUT_SIZE];
        let result = read_stream_to_string(Cursor::new(data));
        assert!(!result.contains(OUTPUT_TRUNCATED_MARKER));
        assert_eq!(result.len(), MAX_OUTPUT_SIZE);
    }

    #[test]
    fn test_max_output_size_is_10mb() {
        assert_eq!(MAX_OUTPUT_SIZE, 10 * 1024 * 1024);
    }
}

#[cfg(all(test, unix))]
#[path = "executor_lifecycle_tests.rs"]
mod lifecycle_tests;
